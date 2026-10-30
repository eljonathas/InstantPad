using InstantPad.Domain.Board;
using Xunit;

namespace InstantPad.Tests.Domain;

public sealed class PadBoardTests
{
    private static InterfaceLayout Layout(int rows, int columns) => new("Memes", rows, columns, "#ffb03a");

    [Fact]
    public void AnEmptyBoardStartsWithOneInterface()
    {
        var board = new PadBoard([], []);

        Assert.Single(board.Interfaces);
    }

    [Fact]
    public void OldBanksBecomeInterfaces()
    {
        var sounds = new[] { new Sound { Slot = 3 }, new Sound { Slot = 25 }, new Sound { Slot = 74 } };

        var board = new PadBoard([], sounds);

        Assert.Equal(["Banco A", "Banco B", "Banco D"], board.Interfaces.Select(item => item.Name));
        Assert.Equal([3, 1, 2], sounds.Select(sound => sound.Slot));
        Assert.All(sounds, sound => Assert.Contains(board.Interfaces, item => item.Id == sound.InterfaceId));
    }

    [Fact]
    public void LoadingAMigratedBoardAddsNothing()
    {
        var first = new PadBoard([], [new Sound { Slot = 3 }]);

        var second = new PadBoard(first.Interfaces, first.Sounds);

        Assert.Single(second.Interfaces);
    }

    [Fact]
    public void SoundsOfAMissingInterfaceAreRecovered()
    {
        var board = new PadBoard([], [new Sound { InterfaceId = "apagada", Slot = 2 }]);

        Assert.Single(board.SoundsOf(board.Interfaces[0]));
    }

    [Fact]
    public void ASmallerGridMovesSoundsIntoFreePads()
    {
        var board = new PadBoard([], []);
        var target = board.Interfaces[0];
        board.Place(target, 0, "a", "a.mp3", "#ff0000");
        board.Place(target, 5, "b", "b.mp3", "#ff0000");
        board.Place(target, 23, "c", "c.mp3", "#ff0000");

        board.Reconfigure(target, Layout(2, 2));

        Assert.Equal([0, 1, 2], board.SoundsOf(target).Select(sound => sound.Slot).Order());
    }

    [Fact]
    public void AGridThatCannotFitTheSoundsIsRefused()
    {
        var board = new PadBoard([], []);
        var target = board.Interfaces[0];
        board.Place(target, 0, "a", "a.mp3", "#ff0000");
        board.Place(target, 1, "b", "b.mp3", "#ff0000");

        Assert.Throws<InvalidOperationException>(() => board.Reconfigure(target, Layout(1, 1)));
    }

    [Fact]
    public void PlacingOnAnOccupiedPadReplacesTheSound()
    {
        var board = new PadBoard([], []);
        var target = board.Interfaces[0];
        board.Place(target, 0, "a", "a.mp3", "#ff0000");

        var placed = board.Place(target, 0, "b", "b.mp3", "#ff0000");

        Assert.Equal(placed, Assert.Single(board.Sounds));
    }

    [Fact]
    public void ANearlyBlackPadTakesAPaletteColor()
    {
        var board = new PadBoard([], []);

        var sound = board.Place(board.Interfaces[0], 1, "a", "a.mp3", "#000000");

        Assert.Equal(PadPalette.PadColorFor(1), sound.Color);
    }

    [Fact]
    public void SwappingExchangesTwoPads()
    {
        var board = new PadBoard([], []);
        var target = board.Interfaces[0];
        var first = board.Place(target, 0, "a", "a.mp3", "#ff0000");
        var second = board.Place(target, 1, "b", "b.mp3", "#ff0000");

        board.Swap(target, 0, 1);

        Assert.Equal(1, first.Slot);
        Assert.Equal(0, second.Slot);
    }

    [Fact]
    public void SendingUsesTheFirstFreePad()
    {
        var board = new PadBoard([], []);
        var source = board.Interfaces[0];
        var destination = board.AddInterface(Layout(1, 2));
        board.Place(destination, 0, "a", "a.mp3", "#ff0000");
        var sound = board.Place(source, 9, "b", "b.mp3", "#ff0000");

        Assert.True(board.SendTo(sound, destination));

        Assert.Equal(destination.Id, sound.InterfaceId);
        Assert.Equal(1, sound.Slot);
    }

    [Fact]
    public void SendingToAFullInterfaceChangesNothing()
    {
        var board = new PadBoard([], []);
        var source = board.Interfaces[0];
        var destination = board.AddInterface(Layout(1, 1));
        board.Place(destination, 0, "a", "a.mp3", "#ff0000");
        var sound = board.Place(source, 9, "b", "b.mp3", "#ff0000");

        Assert.False(board.SendTo(sound, destination));

        Assert.Equal(source.Id, sound.InterfaceId);
    }

    [Fact]
    public void DeletingAnInterfaceRemovesItsSoundsAndPicksANeighbor()
    {
        var board = new PadBoard([], []);
        var first = board.Interfaces[0];
        var second = board.AddInterface(Layout(4, 6));
        board.Place(second, 0, "a", "a.mp3", "#ff0000");

        var next = board.DeleteInterface(second);

        Assert.Equal(first, next);
        Assert.Empty(board.Sounds);
    }

    [Fact]
    public void TheLastInterfaceCannotBeDeleted()
    {
        var board = new PadBoard([], []);

        Assert.Throws<InvalidOperationException>(() => board.DeleteInterface(board.Interfaces[0]));
    }

    [Fact]
    public void NeighborsWrapAround()
    {
        var board = new PadBoard([], []);
        var first = board.Interfaces[0];
        var second = board.AddInterface(Layout(4, 6));

        Assert.Equal(first, board.Neighbor(second, 1));
        Assert.Equal(second, board.Neighbor(first, -1));
    }

    [Fact]
    public void InterfacesCanChangePlaces()
    {
        var board = new PadBoard([], []);
        var first = board.Interfaces[0];
        var second = board.AddInterface(Layout(4, 6));

        board.MoveInterface(second, -1);
        board.MoveInterface(second, -1);

        Assert.Equal([second, first], board.Interfaces);
    }
}
