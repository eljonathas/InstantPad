using System.Runtime.InteropServices;
using System.Security.Cryptography.X509Certificates;
using System.Security.Cryptography;
using InstantPad.Application.Audio;
using InstantPad.Application.Localization;

namespace InstantPad.Infrastructure.VirtualCable;

/// <summary>The driver setup runs elevated, so a tampered download must never reach it.</summary>
public static class VendorSignature
{
    private const string VendorSubject = "CN=BUREL VINCENT";

    private const uint NoUserInterface = 2;
    private const uint VerifyFile = 1;
    private static readonly Guid GenericVerifyAction = new("00AAC56B-CD44-11d0-8CC2-00C04FC295EE");

    /// <exception cref="DriverInstallException">The file is unsigned, or signed by someone else.</exception>
    public static void Verify(string path)
    {
        if (!HasTrustedSignature(path))
            throw new DriverInstallException(Strings.DriverSignatureInvalid);

        var signer = ReadSigner(path);
        if (!signer.Contains(VendorSubject, StringComparison.OrdinalIgnoreCase))
            throw new DriverInstallException(string.Format(Strings.DriverSignerUnexpected, signer));
    }

    private static string ReadSigner(string path)
    {
        try
        {
#pragma warning disable SYSLIB0057 // X509CertificateLoader does not read Authenticode signers.
            using var certificate = new X509Certificate2(X509Certificate.CreateFromSignedFile(path));
#pragma warning restore SYSLIB0057
            return certificate.Subject;
        }
        catch (CryptographicException exception)
        {
            throw new DriverInstallException(Strings.DriverSignatureUnreadable, exception);
        }
    }

    private static bool HasTrustedSignature(string path)
    {
        var fileInfo = new WinTrustFileInfo
        {
            Size = (uint)Marshal.SizeOf<WinTrustFileInfo>(),
            FilePath = path,
        };

        var fileInfoPointer = Marshal.AllocHGlobal(Marshal.SizeOf<WinTrustFileInfo>());
        try
        {
            Marshal.StructureToPtr(fileInfo, fileInfoPointer, false);
            var data = new WinTrustData
            {
                Size = (uint)Marshal.SizeOf<WinTrustData>(),
                UiChoice = NoUserInterface,
                UnionChoice = VerifyFile,
                File = fileInfoPointer,
            };

            var action = GenericVerifyAction;
            return WinVerifyTrust(IntPtr.Zero, ref action, ref data) == 0;
        }
        finally
        {
            Marshal.FreeHGlobal(fileInfoPointer);
        }
    }

    [DllImport("wintrust.dll", CharSet = CharSet.Unicode)]
    private static extern int WinVerifyTrust(IntPtr window, ref Guid action, ref WinTrustData data);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WinTrustFileInfo
    {
        public uint Size;
        public string FilePath;
        public IntPtr FileHandle;
        public IntPtr KnownSubject;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WinTrustData
    {
        public uint Size;
        public IntPtr PolicyCallbackData;
        public IntPtr SipClientData;
        public uint UiChoice;
        public uint RevocationChecks;
        public uint UnionChoice;
        public IntPtr File;
        public uint StateAction;
        public IntPtr StateData;
        public IntPtr UrlReference;
        public uint ProviderFlags;
        public uint UiContext;
        public IntPtr SignatureSettings;
    }
}
