using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Threading;
using InstantPad.Desktop.Services;
using InstantPad.Desktop.ViewModels;

namespace InstantPad.Desktop.Views;

/// <summary>Only what cannot live in a view model: window chrome, drag and drop, and context menus.</summary>
public partial class MainWindow : Window
{
    private const string PadDragFormat = "InstantPad.PadSlot";
    private const int FrameIntervalMs = 33;
    private const int AudioCloseTimeoutMs = 3000;

    private readonly ShellViewModel _shell;
    private readonly SoundRowActions _rowActions;
    private readonly BoardMenus _menus;
    private readonly GlobalHotkeyHost _hotkeyHost;
    private readonly IDisposable _audio;
    private readonly DispatcherTimer _frameTimer = new(DispatcherPriority.Render)
    {
        Interval = TimeSpan.FromMilliseconds(FrameIntervalMs),
    };

    private Point _dragStart;

    public MainWindow(ShellViewModel shell, SoundRowActions rowActions, GlobalHotkeyHost hotkeyHost, IDisposable audio)
    {
        _shell = shell;
        _rowActions = rowActions;
        _menus = new BoardMenus(shell.Board, rowActions);
        _hotkeyHost = hotkeyHost;
        _audio = audio;

        InitializeComponent();
        DataContext = shell;
        hotkeyHost.Attach(this);

        shell.Board.PadTargeted += ShowSoundSources;
        _frameTimer.Tick += (_, _) => shell.UpdateFrame();

        Loaded += OnLoaded;
        Closing += (_, _) => ShutDown();
    }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        _frameTimer.Start();
        await _shell.StartAsync();
    }

    private void ShutDown()
    {
        _frameTimer.Stop();
        _shell.SaveState();
        _hotkeyHost.Dispose();

        // A stuck audio driver must not keep the app from closing.
        Task.Run(_audio.Dispose).Wait(AudioCloseTimeoutMs);
    }

    private void ShowSoundSources()
    {
        if (SetupTab.IsChecked == true)
            BrowseTab.IsChecked = true;
    }

    private void Chassis_MouseDown(object sender, MouseButtonEventArgs e)
    {
        if (e.ButtonState == MouseButtonState.Pressed)
            DragMove();
    }

    private void Minimize_Click(object sender, RoutedEventArgs e) => WindowState = WindowState.Minimized;

    private void Close_Click(object sender, RoutedEventArgs e) => Close();

    private void PanelTab_Checked(object sender, RoutedEventArgs e)
    {
        // The default tab is checked while the XAML is still loading.
        if (BrowsePanel == null || FavoritesPanel == null || SetupPanel == null)
            return;

        BrowsePanel.Visibility = VisibleWhen(BrowseTab);
        FavoritesPanel.Visibility = VisibleWhen(FavoritesTab);
        SetupPanel.Visibility = VisibleWhen(SetupTab);
    }

    private static Visibility VisibleWhen(RadioButton tab) =>
        tab.IsChecked == true ? Visibility.Visible : Visibility.Collapsed;

    private void InterfaceTabs_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (InterfaceTabs.SelectedItem != null)
            InterfaceTabs.ScrollIntoView(InterfaceTabs.SelectedItem);
    }

    private void InterfaceTab_RightClick(object sender, MouseButtonEventArgs e)
    {
        var tab = (FrameworkElement)sender;
        _menus.ForInterface(((InterfaceTabViewModel)tab.DataContext).Model, tab).IsOpen = true;
        e.Handled = true;
    }

    private void Pad_RightClick(object sender, MouseButtonEventArgs e)
    {
        var padButton = (FrameworkElement)sender;
        _menus.ForPad((PadViewModel)padButton.DataContext, padButton).IsOpen = true;
        e.Handled = true;
    }

    private void Pad_PreviewMouseDown(object sender, MouseButtonEventArgs e)
    {
        // Ctrl+drag rearranges pads; a plain press always fires the sound at once.
        var pad = (PadViewModel)((FrameworkElement)sender).DataContext;
        if (Keyboard.Modifiers != ModifierKeys.Control || !pad.HasSound)
            return;

        e.Handled = true;
        DragDrop.DoDragDrop((DependencyObject)sender, new DataObject(PadDragFormat, pad.Slot), DragDropEffects.Move);
    }

    private async void Pad_Drop(object sender, DragEventArgs e)
    {
        var pad = (PadViewModel)((FrameworkElement)sender).DataContext;
        e.Handled = true;

        if (e.Data.GetDataPresent(PadDragFormat))
            _shell.Board.SwapPads((int)e.Data.GetData(PadDragFormat), pad);
        else if (e.Data.GetData(typeof(SoundRowViewModel)) is SoundRowViewModel row)
            await _rowActions.AddToPadAsync(row, pad);
        else if (e.Data.GetData(DataFormats.FileDrop) is string[] { Length: > 0 } files)
            await _shell.Board.ImportFileAsync(files[0], pad);
    }

    private void SoundList_PreviewMouseDown(object sender, MouseButtonEventArgs e) =>
        _dragStart = e.GetPosition(null);

    private void SoundRow_MouseMove(object sender, MouseEventArgs e)
    {
        bool startedOnButton = e.OriginalSource is FrameworkElement { TemplatedParent: Button };
        if (e.LeftButton != MouseButtonState.Pressed || startedOnButton)
            return;

        var moved = e.GetPosition(null) - _dragStart;
        if (Math.Abs(moved.X) < SystemParameters.MinimumHorizontalDragDistance &&
            Math.Abs(moved.Y) < SystemParameters.MinimumVerticalDragDistance)
            return;

        var row = (SoundRowViewModel)((FrameworkElement)sender).DataContext;
        DragDrop.DoDragDrop((DependencyObject)sender, new DataObject(typeof(SoundRowViewModel), row), DragDropEffects.Copy);
    }

    private void SearchBox_GotKeyboardFocus(object sender, KeyboardFocusChangedEventArgs e) => _shell.Hotkeys.Pause();

    private void SearchBox_LostKeyboardFocus(object sender, KeyboardFocusChangedEventArgs e) => _shell.Hotkeys.Resume();

    private void SearchBox_KeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key != Key.Enter)
            return;

        _shell.Browser.SearchCommand.Execute(null);
        Keyboard.ClearFocus();
    }

    // Hands the numeric keys back to the pads as soon as the typing is over.
    private void Search_Click(object sender, RoutedEventArgs e) => Keyboard.ClearFocus();
}
