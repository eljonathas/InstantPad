using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Shapes;
using InstantPad.Desktop.Converters;
using InstantPad.Desktop.ViewModels;
using InstantPad.Domain.Board;
using InstantPad.Domain.Sounds;

namespace InstantPad.Desktop.Views;

/// <summary>Builds the right-click menus of pads and interface tabs.</summary>
public sealed class BoardMenus(BoardViewModel board, SoundRowActions rowActions)
{
    private static readonly int[] VolumePercents = [25, 50, 75, 100, 150];

    public ContextMenu ForPad(PadViewModel pad, UIElement target)
    {
        var menu = new ContextMenu { PlacementTarget = target };

        if (pad.Sound is { } sound)
            AddSoundItems(menu, pad, sound);
        else
            menu.Items.Add(Item("Carregar arquivo de áudio...", () => _ = board.PickFileAsync(pad)));

        return menu;
    }

    public ContextMenu ForInterface(PadInterface target, UIElement placement)
    {
        var menu = new ContextMenu { PlacementTarget = placement };

        menu.Items.Add(Item("Configurar...", () => board.ConfigureInterface(target)));
        menu.Items.Add(Item("Mover para a esquerda", () => board.MoveInterface(target, -1), board.CanMoveInterface(target, -1)));
        menu.Items.Add(Item("Mover para a direita", () => board.MoveInterface(target, 1), board.CanMoveInterface(target, 1)));
        menu.Items.Add(new Separator());
        menu.Items.Add(Item("Excluir interface", () => board.DeleteInterface(target), board.CanDeleteInterface));

        return menu;
    }

    private void AddSoundItems(ContextMenu menu, PadViewModel pad, Sound sound)
    {
        menu.Items.Add(Item(pad.IsFavorite ? "Remover dos favoritos" : "Favoritar", () => _ = rowActions.ToggleFavoriteAsync(ToSource(sound))));
        menu.Items.Add(Item("Baixar no computador...", () => _ = rowActions.SaveToComputerAsync(ToSource(sound))));
        menu.Items.Add(new Separator());

        menu.Items.Add(Item("Definir atalho...", () => board.SetHotkey(pad)));
        if (sound.Hotkey != null)
            menu.Items.Add(Item("Remover atalho", () => board.ClearHotkey(pad)));

        menu.Items.Add(Item("Renomear...", () => board.Rename(pad)));
        menu.Items.Add(VolumeMenu(pad, sound));
        menu.Items.Add(ColorMenu(pad));

        if (board.OtherInterfaces.Any())
            menu.Items.Add(SendToMenu(pad));

        menu.Items.Add(new Separator());
        menu.Items.Add(Item("Remover do pad", () => board.Remove(pad)));
    }

    private static SoundSource ToSource(Sound sound) => new()
    {
        Name = sound.Name,
        Color = sound.Color,
        Path = sound.Path,
    };

    private MenuItem VolumeMenu(PadViewModel pad, Sound sound)
    {
        var menu = new MenuItem { Header = "Volume" };
        foreach (var percent in VolumePercents)
        {
            double volume = percent / 100.0;
            var item = Item($"{percent}%", () => board.SetVolume(pad, volume));
            item.IsChecked = Math.Abs(sound.Volume - volume) < 0.01;
            menu.Items.Add(item);
        }
        return menu;
    }

    private MenuItem ColorMenu(PadViewModel pad)
    {
        var menu = new MenuItem { Header = "Cor" };
        foreach (var hex in PadPalette.Pads)
        {
            var swatch = new Rectangle
            {
                Width = 60,
                Height = 14,
                Fill = new SolidColorBrush(WpfColor.Parse(hex) ?? Colors.Gray),
            };
            var item = new MenuItem { Header = swatch };
            item.Click += (_, _) => board.SetColor(pad, hex);
            menu.Items.Add(item);
        }
        return menu;
    }

    private MenuItem SendToMenu(PadViewModel pad)
    {
        var menu = new MenuItem { Header = "Enviar para interface" };
        foreach (var target in board.OtherInterfaces)
            menu.Items.Add(Item(target.Name, () => board.SendToInterface(pad, target)));
        return menu;
    }

    private static MenuItem Item(string header, Action action, bool isEnabled = true)
    {
        var item = new MenuItem { Header = header, IsEnabled = isEnabled };
        item.Click += (_, _) => action();
        return item;
    }
}
