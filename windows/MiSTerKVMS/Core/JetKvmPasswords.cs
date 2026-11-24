using System.Runtime.InteropServices;
using System.Text;

namespace MiSTerKVMS.Core;

/// <summary>
/// JetKVM passwords in Windows Credential Manager (one generic credential per
/// JetKVM address, protected with the user's login). The core never stores
/// them: it gets them in memory with "setJetkvmPassword" before it opens a
/// JetKVM (docs/JETKVM.md).
/// </summary>
public static class JetKvmPasswords
{
    private const string Prefix = "MiSTerKVMS JetKVM/";
    private const int CRED_TYPE_GENERIC = 1;
    private const int CRED_PERSIST_LOCAL_MACHINE = 2;

    /// <summary>The key a JetKVM is filed under: its address, as the core keys it.</summary>
    public static string Key(string address)
    {
        var a = address.Trim();
        foreach (var p in new[] { "jetkvm://", "http://" })
            if (a.StartsWith(p, StringComparison.OrdinalIgnoreCase)) a = a[p.Length..];
        return a.TrimEnd('/').ToLowerInvariant();
    }

    public static string Load(string address)
    {
        if (!CredRead(Prefix + Key(address), CRED_TYPE_GENERIC, 0, out var ptr)) return "";
        try
        {
            var cred = Marshal.PtrToStructure<Credential>(ptr);
            if (cred.CredentialBlob == IntPtr.Zero || cred.CredentialBlobSize == 0) return "";
            var bytes = new byte[cred.CredentialBlobSize];
            Marshal.Copy(cred.CredentialBlob, bytes, 0, bytes.Length);
            return Encoding.UTF8.GetString(bytes);
        }
        finally
        {
            CredFree(ptr);
        }
    }

    /// <summary>Save (or, empty, delete) the password and hand it to the core.</summary>
    public static void Save(KvmsCore core, string address, string password)
    {
        string target = Prefix + Key(address);
        if (password.Length == 0)
        {
            CredDelete(target, CRED_TYPE_GENERIC, 0);
        }
        else
        {
            var bytes = Encoding.UTF8.GetBytes(password);
            var blob = Marshal.AllocHGlobal(bytes.Length);
            try
            {
                Marshal.Copy(bytes, 0, blob, bytes.Length);
                var cred = new Credential
                {
                    Type = CRED_TYPE_GENERIC,
                    TargetName = target,
                    CredentialBlobSize = bytes.Length,
                    CredentialBlob = blob,
                    Persist = CRED_PERSIST_LOCAL_MACHINE,
                    UserName = Key(address),
                };
                if (!CredWrite(ref cred, 0)) Log.Write($"JetKVM password for {Key(address)}: not saved ({Marshal.GetLastWin32Error()})");
            }
            finally
            {
                Marshal.FreeHGlobal(blob);
            }
        }
        Hand(core, address, password);
    }

    /// <summary>At start-up and when hosts change: the core gets every JetKVM
    /// password the hosts need.</summary>
    public static void HandTo(KvmsCore core, IEnumerable<HostProfile> profiles)
    {
        foreach (var address in profiles.Select(p => p.JetKvmAddress).OfType<string>().Distinct())
            Hand(core, address, Load(address));
    }

    private static void Hand(KvmsCore core, string address, string password)
    {
        try
        {
            core.Call("setJetkvmPassword", new { address = Key(address), password });
        }
        catch (CoreException e)
        {
            Log.Write($"setJetkvmPassword: {e.Message}");
        }
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct Credential
    {
        public int Flags;
        public int Type;
        public string TargetName;
        public string? Comment;
        public long LastWritten;
        public int CredentialBlobSize;
        public IntPtr CredentialBlob;
        public int Persist;
        public int AttributeCount;
        public IntPtr Attributes;
        public string? TargetAlias;
        public string? UserName;
    }

    [DllImport("advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredRead(string target, int type, int flags, out IntPtr credential);

    [DllImport("advapi32.dll", EntryPoint = "CredWriteW", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredWrite(ref Credential credential, int flags);

    [DllImport("advapi32.dll", EntryPoint = "CredDeleteW", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredDelete(string target, int type, int flags);

    [DllImport("advapi32.dll")]
    private static extern void CredFree(IntPtr buffer);
}
