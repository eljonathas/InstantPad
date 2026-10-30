using System.Windows;
using InstantPad.Desktop.Services;

namespace InstantPad.Desktop;

public partial class App : System.Windows.Application
{
    private readonly ApplicationRestarter _restarter = new();
    private readonly SingleInstanceGuard _instanceGuard = new();

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        if (!_instanceGuard.IsFirstInstance)
        {
            _instanceGuard.ShowFirstInstance();
            Shutdown();
            return;
        }

        DispatcherUnhandledException += (_, unhandled) =>
        {
            MessageBox.Show(unhandled.Exception.Message, "InstantPad", MessageBoxButton.OK, MessageBoxImage.Error);
            unhandled.Handled = true;
        };

        MainWindow = CompositionRoot.CreateMainWindow(_restarter);
        MainWindow.Show();
        _instanceGuard.ListenForShowRequests(() => Dispatcher.BeginInvoke(ShowMainWindow));
    }

    protected override void OnExit(ExitEventArgs e)
    {
        base.OnExit(e);
        _instanceGuard.Dispose();
        _restarter.StartNewProcessIfRequested();
    }

    private void ShowMainWindow()
    {
        if (MainWindow.WindowState == WindowState.Minimized)
            MainWindow.WindowState = WindowState.Normal;

        MainWindow.Show();
        MainWindow.Activate();
    }
}
