using System.ComponentModel;
using System.Globalization;
using InstantPad.Application.Localization;

namespace InstantPad.Desktop.Localization;

/// <summary>Holds the interface language. Bindings to its indexer update when the language changes.</summary>
public sealed class Localizer : INotifyPropertyChanged
{
    private const string IndexerProperty = "Item[]";

    private static readonly CultureInfo SystemLanguage = CultureInfo.CurrentUICulture;

    private Localizer()
    {
    }

    public static Localizer Instance { get; } = new();

    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>Raised after the language changes, for text that view models build themselves.</summary>
    public event Action? LanguageChanged;

    public string this[string key] => Strings.ResourceManager.GetString(key, CultureInfo.CurrentUICulture) ?? key;

    /// <param name="languageTag">A tag such as "en" or "pt". Null follows the system language.</param>
    public void Apply(string? languageTag)
    {
        var culture = languageTag == null ? SystemLanguage : CultureInfo.GetCultureInfo(languageTag);

        CultureInfo.CurrentUICulture = culture;
        CultureInfo.DefaultThreadCurrentUICulture = culture;

        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(IndexerProperty));
        LanguageChanged?.Invoke();
    }
}
