namespace InstantPad.Domain.Board;

public static class PadPalette
{
    public const string Neutral = "#3a3f4b";

    private const double NearlyBlackLuminance = 0.12;

    public static IReadOnlyList<string> Pads { get; } =
        ["#ff3b5c", "#ff8a1f", "#ffd21f", "#35f27a", "#1fd6ff", "#4f7dff", "#b45cff", "#ff5cc8"];

    public static IReadOnlyList<string> Accents { get; } =
        ["#ffb03a", "#35f27a", "#1fd6ff", "#4f7dff", "#b45cff", "#ff5cc8", "#ff3b5c", "#e8e8ea"];

    public static string PadColorFor(int slot) => Pads[slot % Pads.Count];

    public static string AccentFor(int index) => Accents[index % Accents.Count];

    /// <summary>A nearly black pad would look switched off, so it takes a palette color instead.</summary>
    public static string VisibleColor(string color, int slot) =>
        HexColor.Luminance(color) < NearlyBlackLuminance ? PadColorFor(slot) : color;
}
