using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using InstantPad.Application.Hotkeys;
using InstantPad.Domain.Hotkeys;

namespace InstantPad.Desktop.Services;

/// <summary>System-wide hotkeys, so sounds fire while a game has focus.</summary>
public sealed class GlobalHotkeyHost : IHotkeyHost, IDisposable
{
    private const int HotkeyMessage = 0x0312;
    private const uint NoRepeat = 0x4000;

    private readonly Dictionary<int, Action> _actions = [];
    private HwndSource? _source;
    private int _nextId = 1;

    /// <summary>Hotkey messages arrive through a window, so this must run before the first registration.</summary>
    public void Attach(Window window)
    {
        _source = HwndSource.FromHwnd(new WindowInteropHelper(window).EnsureHandle());
        _source.AddHook(OnWindowMessage);
    }

    public bool Register(Hotkey hotkey, Action action)
    {
        int id = _nextId++;
        if (!RegisterHotKey(_source!.Handle, id, (uint)hotkey.Modifiers | NoRepeat, (uint)hotkey.VirtualKey))
            return false;

        _actions[id] = action;
        return true;
    }

    public void Clear()
    {
        if (_source == null)
            return;

        foreach (var id in _actions.Keys)
            UnregisterHotKey(_source.Handle, id);

        _actions.Clear();
    }

    public void Dispose()
    {
        Clear();
        _source?.RemoveHook(OnWindowMessage);
    }

    private IntPtr OnWindowMessage(IntPtr window, int message, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        if (message == HotkeyMessage && _actions.TryGetValue(wParam.ToInt32(), out var action))
        {
            action();
            handled = true;
        }
        return IntPtr.Zero;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool RegisterHotKey(IntPtr window, int id, uint modifiers, uint virtualKey);

    [DllImport("user32.dll")]
    private static extern bool UnregisterHotKey(IntPtr window, int id);
}
