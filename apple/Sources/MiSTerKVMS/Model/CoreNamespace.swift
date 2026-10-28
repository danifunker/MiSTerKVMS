#if !canImport(KVMSCore)
/// The App Store build (apple/MiSTerKVMS.xcodeproj) compiles the core's
/// bindings into the app instead of a KVMSCore module: the module-qualified
/// calls below still work.
enum KVMSCore {
    static func captureCards() -> [CaptureCard] { MiSTerKVMS.captureCards() }
    static func audioInputs() -> [AudioInput] { MiSTerKVMS.audioInputs() }
}
#endif
