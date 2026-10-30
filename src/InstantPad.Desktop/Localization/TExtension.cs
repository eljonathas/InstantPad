using System.Windows.Data;
using System.Windows.Markup;

namespace InstantPad.Desktop.Localization;

/// <summary>Translated text in XAML: <c>Text="{l:T Search}"</c>. It follows language changes at once.</summary>
[MarkupExtensionReturnType(typeof(object))]
public sealed class TExtension(string key) : MarkupExtension
{
    [ConstructorArgument("key")]
    public string Key { get; } = key;

    public override object ProvideValue(IServiceProvider serviceProvider)
    {
        var binding = new Binding($"[{Key}]") { Source = Localizer.Instance, Mode = BindingMode.OneWay };
        return binding.ProvideValue(serviceProvider);
    }
}
