namespace InstantPad.Desktop.Services;

/// <summary>
/// Keeps one copy of the app running, because a second one could not register the hotkeys.
/// Opening it again brings the running window to the front.
/// </summary>
public sealed class SingleInstanceGuard : IDisposable
{
    // The installer watches this name too, to ask for the app to close. See build/installer.iss.
    private const string MutexName = "InstantPad.SingleInstance";
    private const string ShowWindowEventName = "InstantPad.ShowWindow";

    private readonly Mutex _mutex;
    private readonly EventWaitHandle _showWindow;
    private RegisteredWaitHandle? _listener;

    public SingleInstanceGuard()
    {
        _mutex = new Mutex(initiallyOwned: true, MutexName, out bool createdNew);
        _showWindow = new EventWaitHandle(false, EventResetMode.AutoReset, ShowWindowEventName);
        IsFirstInstance = createdNew;
    }

    public bool IsFirstInstance { get; }

    /// <summary>Asks the running copy to show its window.</summary>
    public void ShowFirstInstance() => _showWindow.Set();

    public void ListenForShowRequests(Action showWindow)
    {
        _listener = ThreadPool.RegisterWaitForSingleObject(
            _showWindow, (_, _) => showWindow(), null, Timeout.Infinite, executeOnlyOnce: false);
    }

    /// <summary>Must run before a restart starts the new process, which would otherwise see this one.</summary>
    public void Dispose()
    {
        _listener?.Unregister(null);
        if (IsFirstInstance)
            _mutex.ReleaseMutex();

        _mutex.Dispose();
        _showWindow.Dispose();
    }
}
