import SwiftUI

/// A demo host's input over the picture: what a MiSTer would have
/// received, newest last, each line gone after 5 seconds (the core keeps
/// the list: core/src/demo.rs).
struct DemoInputList: View {
    @EnvironmentObject private var controller: SessionController

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.15)) { _ in
            let lines = controller.demoInput()
            VStack(alignment: .leading, spacing: 2) {
                Text("Demo: what the MiSTer would receive")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                if lines.isEmpty {
                    Text("Click the picture, then move, click and type…")
                        .font(.callout.monospaced())
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                        Text(line).font(.callout.monospaced())
                    }
                }
            }
            .padding(.horizontal, 10).padding(.vertical, 8)
            .background(.black.opacity(0.6), in: RoundedRectangle(cornerRadius: 8))
            .foregroundStyle(.white)
            .allowsHitTesting(false)
        }
    }
}
