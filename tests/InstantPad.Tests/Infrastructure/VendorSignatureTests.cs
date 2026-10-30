using System.IO;
using InstantPad.Application.Audio;
using InstantPad.Infrastructure.VirtualCable;
using Xunit;

namespace InstantPad.Tests.Infrastructure;

public sealed class VendorSignatureTests
{
    [Fact]
    public void AnUnsignedFileIsRefused()
    {
        var path = Path.GetTempFileName();
        try
        {
            File.WriteAllBytes(path, [0x4D, 0x5A, 0, 0]);

            Assert.Throws<DriverInstallException>(() => VendorSignature.Verify(path));
        }
        finally
        {
            File.Delete(path);
        }
    }
}
