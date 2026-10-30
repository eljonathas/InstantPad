namespace InstantPad.Domain.Board;

/// <summary>All interfaces and the sounds on their pads, with the rules that keep them consistent.</summary>
public sealed class PadBoard
{
    private const int LegacyPadsPerBank = 24;

    private readonly List<PadInterface> _interfaces;
    private readonly List<Sound> _sounds;

    public PadBoard(IEnumerable<PadInterface> interfaces, IEnumerable<Sound> sounds, string defaultInterfaceName = "Main")
    {
        _interfaces = interfaces.ToList();
        _sounds = sounds.ToList();

        AdoptOrphanSounds();
        if (_interfaces.Count == 0)
            _interfaces.Add(new PadInterface { Name = defaultInterfaceName });
    }

    public IReadOnlyList<PadInterface> Interfaces => _interfaces;

    public IReadOnlyList<Sound> Sounds => _sounds;

    public IReadOnlyList<Sound> SoundsOf(PadInterface target) =>
        _sounds.Where(sound => sound.InterfaceId == target.Id).ToList();

    public Sound? SoundAt(PadInterface target, int slot) =>
        _sounds.FirstOrDefault(sound => sound.InterfaceId == target.Id && sound.Slot == slot);

    public int? FirstFreeSlot(PadInterface target)
    {
        var used = SoundsOf(target).Select(sound => sound.Slot).ToHashSet();
        var free = Enumerable.Range(0, target.Capacity).Where(slot => !used.Contains(slot));
        return free.Cast<int?>().FirstOrDefault();
    }

    public PadInterface AddInterface(InterfaceLayout layout)
    {
        var created = new PadInterface();
        created.Apply(layout);
        _interfaces.Add(created);
        return created;
    }

    public void Reconfigure(PadInterface target, InterfaceLayout layout)
    {
        var sounds = SoundsOf(target);
        if (layout.Capacity < sounds.Count)
            throw new InvalidOperationException($"{layout.Capacity} pads do not fit {sounds.Count} sounds.");

        target.Apply(layout);
        MoveOverflowIntoFreeSlots(sounds, layout.Capacity);
    }

    /// <summary>Removes the interface and its sounds. Returns the interface to show in its place.</summary>
    public PadInterface DeleteInterface(PadInterface target)
    {
        if (_interfaces.Count == 1)
            throw new InvalidOperationException("The board needs at least one interface.");

        int index = _interfaces.IndexOf(target);
        _sounds.RemoveAll(sound => sound.InterfaceId == target.Id);
        _interfaces.Remove(target);
        return _interfaces[Math.Min(index, _interfaces.Count - 1)];
    }

    public void MoveInterface(PadInterface target, int offset)
    {
        int from = _interfaces.IndexOf(target);
        int to = from + offset;
        if (to < 0 || to >= _interfaces.Count)
            return;

        _interfaces.RemoveAt(from);
        _interfaces.Insert(to, target);
    }

    /// <summary>The interface <paramref name="offset"/> positions away, wrapping at both ends.</summary>
    public PadInterface Neighbor(PadInterface target, int offset)
    {
        int count = _interfaces.Count;
        return _interfaces[(_interfaces.IndexOf(target) + offset + count) % count];
    }

    /// <summary>Puts a new sound on a pad, replacing whatever was there.</summary>
    public Sound Place(PadInterface target, int slot, string name, string path, string color)
    {
        if (SoundAt(target, slot) is { } previous)
            _sounds.Remove(previous);

        var sound = new Sound
        {
            Name = name,
            Path = path,
            Color = PadPalette.VisibleColor(color, slot),
            InterfaceId = target.Id,
            Slot = slot,
        };
        _sounds.Add(sound);
        return sound;
    }

    public void Remove(Sound sound) => _sounds.Remove(sound);

    /// <summary>Moves a sound to another pad, swapping with the sound already there.</summary>
    public void Swap(PadInterface target, int fromSlot, int toSlot)
    {
        var moving = SoundAt(target, fromSlot);
        if (moving == null || fromSlot == toSlot)
            return;

        if (SoundAt(target, toSlot) is { } displaced)
            displaced.Slot = fromSlot;
        moving.Slot = toSlot;
    }

    /// <summary>Returns false when the target interface has no free pad.</summary>
    public bool SendTo(Sound sound, PadInterface target)
    {
        if (FirstFreeSlot(target) is not { } free)
            return false;

        sound.InterfaceId = target.Id;
        sound.Slot = free;
        return true;
    }

    /// <summary>
    /// Sounds saved with the old fixed banks become one interface per bank. The same path
    /// recovers sounds whose interface is gone, so a damaged file never hides them.
    /// </summary>
    private void AdoptOrphanSounds()
    {
        var known = _interfaces.Select(item => item.Id).ToHashSet();
        var orphans = _sounds.Where(sound => sound.InterfaceId == null || !known.Contains(sound.InterfaceId));

        foreach (var bank in orphans.GroupBy(sound => sound.Slot / LegacyPadsPerBank).OrderBy(group => group.Key).ToList())
        {
            var adopted = new PadInterface { Name = $"Banco {(char)('A' + bank.Key)}" };
            _interfaces.Add(adopted);

            foreach (var sound in bank)
            {
                sound.InterfaceId = adopted.Id;
                sound.Slot %= LegacyPadsPerBank;
            }
        }
    }

    private static void MoveOverflowIntoFreeSlots(IReadOnlyCollection<Sound> sounds, int capacity)
    {
        var used = sounds.Where(sound => sound.Slot < capacity).Select(sound => sound.Slot).ToHashSet();
        int free = 0;

        foreach (var sound in sounds.Where(sound => sound.Slot >= capacity).OrderBy(sound => sound.Slot))
        {
            while (used.Contains(free))
                free++;

            sound.Slot = free;
            used.Add(free);
        }
    }
}
