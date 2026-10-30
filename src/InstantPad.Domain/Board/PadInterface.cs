namespace InstantPad.Domain.Board;

/// <summary>A pad layout of its own. The user can create as many as they want.</summary>
public sealed class PadInterface
{
    public const int MaxRows = 8;
    public const int MaxColumns = 10;

    public string Id { get; set; } = Guid.NewGuid().ToString("N");
    public string Name { get; set; } = "";
    public int Rows { get; set; } = 4;
    public int Columns { get; set; } = 6;
    public string Accent { get; set; } = PadPalette.Accents[0];

    public int Capacity => Rows * Columns;

    public InterfaceLayout ToLayout() => new(Name, Rows, Columns, Accent);

    public void Apply(InterfaceLayout layout)
    {
        Name = layout.Name;
        Rows = layout.Rows;
        Columns = layout.Columns;
        Accent = layout.Accent;
    }
}

/// <summary>The part of an interface the user configures.</summary>
public sealed record InterfaceLayout(string Name, int Rows, int Columns, string Accent)
{
    public int Capacity => Rows * Columns;
}
