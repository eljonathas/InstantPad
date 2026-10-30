using InstantPad.Domain.Board;
using InstantPad.Domain.Hotkeys;

namespace InstantPad.Application.Hotkeys;

/// <summary>Keeps the system-wide keys in sync with the board and the numeric scheme.</summary>
public sealed class HotkeyService(IHotkeyHost host)
{
    private int _pauseCount;
    private IReadOnlyList<Binding> _bindings = [];

    public event Action<IReadOnlyList<Hotkey>>? RegistrationFailed;

    /// <summary>
    /// A key can only do one thing, so the first binding wins:
    /// keys set by hand on a pad, then the stop key, then the numeric scheme.
    /// </summary>
    public static IReadOnlyList<Binding> Resolve(
        IEnumerable<Sound> sounds, NumericLayout layout, Hotkey? customStop, IHotkeyActions actions)
    {
        var bindings = new Dictionary<Hotkey, Action>();

        foreach (var sound in sounds.Where(sound => sound.Hotkey != null))
            bindings.TryAdd(sound.Hotkey!, () => actions.PlaySound(sound));

        if ((customStop ?? layout.StopAll) is { } stop)
            bindings.TryAdd(stop, actions.StopAll);

        for (int index = 0; index < layout.Pads.Count; index++)
        {
            int padIndex = index;
            bindings.TryAdd(layout.Pads[index], () => actions.PlayPad(padIndex));
        }

        if (layout.NextInterface is { } next)
            bindings.TryAdd(next, actions.NextInterface);
        if (layout.PreviousInterface is { } previous)
            bindings.TryAdd(previous, actions.PreviousInterface);

        return bindings.Select(binding => new Binding(binding.Key, binding.Value)).ToList();
    }

    public void Apply(IReadOnlyList<Binding> bindings)
    {
        _bindings = bindings;
        if (_pauseCount == 0)
            RegisterAll();
    }

    /// <summary>Global keys swallow their presses, so they must be off while the user types.</summary>
    public void Pause()
    {
        _pauseCount++;
        host.Clear();
    }

    public void Resume()
    {
        _pauseCount--;
        if (_pauseCount == 0)
            RegisterAll();
    }

    private void RegisterAll()
    {
        host.Clear();

        var failed = _bindings
            .Where(binding => !host.Register(binding.Hotkey, binding.Action))
            .Select(binding => binding.Hotkey)
            .ToList();

        if (failed.Count > 0)
            RegistrationFailed?.Invoke(failed);
    }

    public sealed record Binding(Hotkey Hotkey, Action Action);
}
