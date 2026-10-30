namespace InstantPad.Application.Updates;

public interface IApplicationRestarter
{
    /// <summary>Closes the running app and opens the executable that is now installed.</summary>
    void Restart();
}
