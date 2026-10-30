namespace InstantPad.Infrastructure.Audio;

/// <summary>
/// Hands samples from the capture thread to the render thread. The two devices run on
/// different clocks, so it drops the backlog when latency builds up.
/// </summary>
public sealed class MicRingBuffer
{
    private static readonly int TargetSamples = AudioFormat.SamplesFor(30);
    private static readonly int MaxSamples = AudioFormat.SamplesFor(120);

    private readonly object _lock = new();
    private readonly float[] _buffer = new float[AudioFormat.SamplesFor(1000)];
    private int _readPosition;
    private int _count;
    private bool _primed;

    public void Write(float[] samples, int count)
    {
        lock (_lock)
        {
            int writePosition = (_readPosition + _count) % _buffer.Length;
            for (int index = 0; index < count; index++)
            {
                _buffer[writePosition] = samples[index];
                writePosition = (writePosition + 1) % _buffer.Length;
            }

            _count += count;
            if (_count > _buffer.Length)
            {
                _readPosition = writePosition;
                _count = _buffer.Length;
            }

            if (_count > MaxSamples)
                DropOldest(_count - TargetSamples);
        }
    }

    public int Read(float[] destination, int count)
    {
        lock (_lock)
        {
            if (!_primed)
            {
                if (_count < TargetSamples)
                    return 0;
                _primed = true;
            }

            int read = Math.Min(count, _count);
            for (int index = 0; index < read; index++)
            {
                destination[index] = _buffer[_readPosition];
                _readPosition = (_readPosition + 1) % _buffer.Length;
            }
            _count -= read;

            // Ran dry: wait for the prebuffer again instead of crackling sample by sample.
            if (read < count)
                _primed = false;

            return read;
        }
    }

    public void Reset()
    {
        lock (_lock)
        {
            _readPosition = 0;
            _count = 0;
            _primed = false;
        }
    }

    private void DropOldest(int samples)
    {
        // Whole frames only, or left and right would swap.
        samples -= samples % AudioFormat.Channels;
        _readPosition = (_readPosition + samples) % _buffer.Length;
        _count -= samples;
    }
}
