import SwiftUI

/// The color themes of `design/themes.json` (the desktop's): on iOS each
/// theme gives the accent color and, for Notte, dark mode. The rest of the
/// colors are the system's, so the app looks like the other iPhone apps.
struct Theme: Identifiable {
    let id: String
    /// Translation key of the name.
    let nameKey: String
    let light: Color
    /// Accent in dark mode (Notte's), for the light themes when the phone is dark.
    let dark: Color
    let scheme: ColorScheme?
    let unreadDot: Color

    var accent: Color { Color(light: light, dark: dark) }

    /// "auto": Ruggine, Notte in dark mode.
    static let auto = Theme(
        id: "auto", nameKey: "theme_auto", light: hex(0xA63A0C), dark: hex(0xE07A4A), scheme: nil,
        unreadDot: Color(light: hex(0xA63A0C), dark: hex(0xE07A4A))
    )

    static let all: [Theme] = [
        auto,
        light("ruggine", 0xA63A0C),
        light("ardesia", 0x1E5AA8),
        light("bosco", 0x2F6B45),
        light("lavanda", 0x6B4FA0),
        light("sabbia", 0x8A5A12),
        Theme(id: "notte", nameKey: "theme_notte", light: hex(0xE07A4A), dark: hex(0xE07A4A), scheme: .dark,
              unreadDot: hex(0xE07A4A)),
    ]

    static func current(_ id: String) -> Theme { all.first { $0.id == id } ?? auto }

    private static func light(_ id: String, _ accent: UInt32) -> Theme {
        Theme(id: id, nameKey: "theme_\(id)", light: hex(accent), dark: hex(0xE07A4A), scheme: .light,
              unreadDot: hex(accent))
    }

    static func hex(_ v: UInt32) -> Color {
        Color(red: Double((v >> 16) & 0xFF) / 255, green: Double((v >> 8) & 0xFF) / 255, blue: Double(v & 0xFF) / 255)
    }

    /// "#RRGGBB" or "#RRGGBBAA" (calendar colors); nil when empty or invalid.
    static func parse(_ text: String) -> Color? {
        var s = text.trimmingCharacters(in: .whitespaces)
        guard s.hasPrefix("#") else { return nil }
        s.removeFirst()
        guard s.count == 6 || s.count == 8, let v = UInt32(s, radix: 16) else { return nil }
        return s.count == 6 ? hex(v) : hex(v >> 8).opacity(Double(v & 0xFF) / 255)
    }
}

extension Color {
    init(light: Color, dark: Color) {
        self.init(UIColor { $0.userInterfaceStyle == .dark ? UIColor(dark) : UIColor(light) })
    }
}

/// The dot of a theme in the picker: its own color whatever the phone's
/// mode; "auto" is half Ruggine, half Notte.
struct ThemeSwatch: View {
    let theme: Theme

    var body: some View {
        Group {
            if theme.scheme == nil {
                Circle()
                    .fill(theme.light)
                    .overlay(Circle().trim(from: 0.25, to: 0.75).fill(theme.dark).rotationEffect(.degrees(180)))
            } else {
                Circle().fill(theme.light)
            }
        }
        .frame(width: 14, height: 14)
        .accessibilityHidden(true)
    }
}
