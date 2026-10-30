using System.Diagnostics;
using InstantPad.Application.Updates;

namespace InstantPad.Desktop.Services;

/// <summary>
/// Starts the new process only after this one has released its hotkeys,
/// or the new instance would fail to register them.
/// </summary>
public sealed class ApplicationRestarter : IApplicationRestarter
{
    private bool _requested;

    public void Restart()
    {
        _requested = true;
        System.Windows.Application.Current.Shutdown();
    }

    public void StartNewProcessIfRequested()
    {
        if (_requested)
            Process.Start(new ProcessStartInfo(Environment.ProcessPath!) { UseShellExecute = true });
    }
}
