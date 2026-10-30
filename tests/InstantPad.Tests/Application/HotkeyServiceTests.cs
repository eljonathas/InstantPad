using InstantPad.Application.Hotkeys;
using InstantPad.Domain.Board;
using InstantPad.Domain.Hotkeys;
using Xunit;

namespace InstantPad.Tests.Application;

public sealed class HotkeyServiceTests
{
    private const int KeysInNumericScheme = 13;

    private static readonly NumericLayout Numpad = NumericShortcuts.LayoutFor(NumericScheme.Numpad);

    private readonly RecordingActions _actions = new();
    private readonly FakeHost _host = new();

    private static Action ActionFor(IEnumerable<HotkeyService.Binding> bindings, Hotkey key) =>
        bindings.Single(binding => binding.Hotkey == key).Action;

    [Fact]
    public void TheNumericSchemeBindsPadsNavigationAndStop()
    {
        var bindings = HotkeyService.Resolve([], Numpad, null, _actions);

        Assert.Equal(KeysInNumericScheme, bindings.Count);
        ActionFor(bindings, Numpad.Pads[2])();
        Assert.Equal(2, _actions.PlayedPad);
    }

    [Fact]
    public void AKeySetOnAPadWinsOverTheNumericScheme()
    {
        var sound = new Sound { Name = "meu", Hotkey = Numpad.Pads[0] };

        var bindings = HotkeyService.Resolve([sound], Numpad, null, _actions);

        ActionFor(bindings, Numpad.Pads[0])();
        Assert.Equal(sound, _actions.PlayedSound);
        Assert.Null(_actions.PlayedPad);
    }

    [Fact]
    public void ACustomStopKeyReplacesTheOneFromTheScheme()
    {
        var customStop = new Hotkey(VirtualKeys.F1);

        var bindings = HotkeyService.Resolve([], Numpad, customStop, _actions);

        Assert.Contains(bindings, binding => binding.Hotkey == customStop);
        Assert.DoesNotContain(bindings, binding => binding.Hotkey == Numpad.StopAll);
    }

    [Fact]
    public void KeysStayOffWhilePausedAndComeBackAfterTheLastResume()
    {
        var service = new HotkeyService(_host);
        service.Apply(HotkeyService.Resolve([], Numpad, null, _actions));

        service.Pause();
        service.Pause();
        service.Resume();
        Assert.Empty(_host.Registered);

        service.Resume();
        Assert.Equal(KeysInNumericScheme, _host.Registered.Count);
    }

    [Fact]
    public void KeysAppliedWhilePausedWaitForTheResume()
    {
        var service = new HotkeyService(_host);

        service.Pause();
        service.Apply(HotkeyService.Resolve([], Numpad, null, _actions));
        Assert.Empty(_host.Registered);

        service.Resume();
        Assert.Equal(KeysInNumericScheme, _host.Registered.Count);
    }

    [Fact]
    public void KeysOwnedByAnotherProgramAreReported()
    {
        _host.Refused = Numpad.Pads[0];
        var service = new HotkeyService(_host);
        IReadOnlyList<Hotkey>? failed = null;
        service.RegistrationFailed += keys => failed = keys;

        service.Apply(HotkeyService.Resolve([], Numpad, null, _actions));

        Assert.Equal([Numpad.Pads[0]], failed);
    }

    private sealed class FakeHost : IHotkeyHost
    {
        public Hotkey? Refused { get; set; }

        public List<Hotkey> Registered { get; } = [];

        public bool Register(Hotkey hotkey, Action action)
        {
            if (hotkey == Refused)
                return false;

            Registered.Add(hotkey);
            return true;
        }

        public void Clear() => Registered.Clear();
    }

    private sealed class RecordingActions : IHotkeyActions
    {
        public Sound? PlayedSound { get; private set; }

        public int? PlayedPad { get; private set; }

        public void PlaySound(Sound sound) => PlayedSound = sound;

        public void PlayPad(int padIndex) => PlayedPad = padIndex;

        public void NextInterface() { }

        public void PreviousInterface() { }

        public void StopAll() { }
    }
}
