namespace InstantPad.Application.Updates;

/// <summary>An update did not complete. The message is ready to show to the user.</summary>
public sealed class UpdateFailedException : Exception
{
    public UpdateFailedException(string message, Exception? innerException = null)
        : base(message, innerException)
    {
    }
}
