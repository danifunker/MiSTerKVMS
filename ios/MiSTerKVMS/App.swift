import SwiftUI

@main
struct MiSTerKVMSApp: App {
    @UIApplicationDelegateAdaptor private var appDelegate: AppDelegate
    @StateObject private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .background: model.suspend()
            case .active: model.resume()
            default: break
            }
        }
    }
}

/// Portrait on a phone, as designed (IPHONE.md P1), except on a screen
/// close to square, a foldable such as the iPhone Duo open, where landscape
/// gives the picture twice the room. Every orientation on iPad.
final class AppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, supportedInterfaceOrientationsFor window: UIWindow?) -> UIInterfaceOrientationMask {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return .all }
        guard let size = window?.windowScene?.screen.bounds.size, Self.isSquarish(size) else { return .portrait }
        return .allButUpsideDown
    }

    static func isSquarish(_ size: CGSize) -> Bool {
        min(size.width, size.height) / max(size.width, size.height) > 0.6
    }
}

struct RootView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        Group {
            if model.showWelcome {
                WelcomeView()
            } else {
                NavigationStack {
                    HostListView()
                }
            }
        }
        .fullScreenCover(isPresented: Binding(
            get: { model.state.profileId != nil },
            set: { if !$0 { model.close() } }
        )) {
            SessionScreen()
        }
        // Folding or unfolding a foldable changes what's allowed: ask again.
        .onGeometryChange(for: Bool.self) { AppDelegate.isSquarish($0.size) } action: { _ in
            for scene in UIApplication.shared.connectedScenes {
                for window in (scene as? UIWindowScene)?.windows ?? [] {
                    window.rootViewController?.setNeedsUpdateOfSupportedInterfaceOrientations()
                }
            }
        }
    }
}
