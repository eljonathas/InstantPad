using System.Windows;
using InstantPad.Desktop.Services;

namespace InstantPad.Desktop;

public partial class App : System.Windows.Application
{
    private readonly ApplicationRestarter _restarter = new();

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        DispatcherUnhandledException += (_, unhandled) =>
        {
            MessageBox.Show(unhandled.Exception.Message, "InstantPad", MessageBoxButton.OK, MessageBoxImage.Error);
            unhandled.Handled = true;
        };

        MainWindow = CompositionRoot.CreateMainWindow(_restarter);
        MainWindow.Show();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        base.OnExit(e);
        _restarter.StartNewProcessIfRequested();
    }
}
