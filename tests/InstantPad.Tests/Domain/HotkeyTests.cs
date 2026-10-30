using InstantPad.Domain.Board;
using InstantPad.Domain.Hotkeys;
using Xunit;

namespace InstantPad.Tests.Domain;

public sealed class HotkeyTests
{
    [Theory]
    [InlineData(NumericScheme.Numpad, "Num1", "Num0", "Num+", "Num-", "Num,")]
    [InlineData(NumericScheme.CtrlNumpad, "Ctrl+Num1", "Ctrl+Num0", "Ctrl+Num+", "Ctrl+Num-", "Ctrl+Num,")]
    [InlineData(NumericScheme.AltDigits, "Alt+1", "Alt+0", "Alt+=", "Alt+-", "Alt+.")]
    public void NumericSchemesCoverTenPadsAndNavigation(
        NumericScheme scheme, string firstPad, string lastPad, string next, string previous, string stop)
    {
        var layout = NumericShortcuts.LayoutFor(scheme);

        Assert.Equal(10, layout.Pads.Count);
        Assert.Equal(firstPad, layout.Pads[0].ToString());
        Assert.Equal(lastPad, layout.Pads[9].ToString());
        Assert.Equal(next, layout.NextInterface?.ToString());
        Assert.Equal(previous, layout.PreviousInterface?.ToString());
        Assert.Equal(stop, layout.StopAll?.ToString());
    }

    [Fact]
    public void TheOffSchemeHasNoKeys()
    {
        var layout = NumericShortcuts.LayoutFor(NumericScheme.Off);

        Assert.Empty(layout.Pads);
        Assert.Null(layout.StopAll);
    }

    [Theory]
    [InlineData(0x7B, HotkeyModifiers.Control | HotkeyModifiers.Alt, "Ctrl+Alt+F12")]
    [InlineData(0x41, HotkeyModifiers.Shift, "Shift+A")]
    [InlineData(0x60, HotkeyModifiers.None, "Num0")]
    public void HotkeysReadLikeOnTheKeyboard(int key, HotkeyModifiers modifiers, string expected)
    {
        Assert.Equal(expected, new Hotkey(key, modifiers).ToString());
    }

    [Theory]
    [InlineData("#000000", 0)]
    [InlineData("#ffffff", 1)]
    [InlineData("#fff", 1)]
    [InlineData("#ff000000", 0)]
    [InlineData("invalida", 0)]
    [InlineData(null, 0)]
    public void LuminanceReadsTheHexFormsTheSiteUses(string? hex, double expected)
    {
        Assert.Equal(expected, HexColor.Luminance(hex), precision: 3);
    }
}
