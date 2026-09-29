import Foundation

// Local notifications of the background check for two things the app already computes but only showed inside it:
// the configuration changes of the watched assets (ConfigChanges) and the positions that became dangerous
// (RiskEngine.dangerousPositions). What to notify given the previous and the new state, the keys that prevent a second
// notification of the same thing, and the texts (the web cards' wording). Pure; the app schedules the notifications.

/// A notification to post: identifier, texts and the asset to open when tapped ("crypto:BTC"; nil: the app opens).
public struct LocalNotice: Sendable, Equatable {
    public var id: String
    public var title: String
    public var body: String
    public var asset: String?

    public init(id: String, title: String, body: String, asset: String?) {
        self.id = id
        self.title = title
        self.body = body
        self.asset = asset
    }
}

/// What was already notified, kept on the iPhone.
public struct ChangeNoticeState: Codable, Sendable, Equatable {
    /// Ids of the configuration changes already notified (oldest first, at most `ChangeNotices.maxRemembered`).
    public var configNotified: [String] = []
    /// Danger keys ("line id:code") present at the last measurement; nil: never measured (a baseline is taken).
    public var dangerActive: [String]? = nil
    /// When each danger key was last notified (ms): a danger that disappears and comes back within
    /// `ChangeNotices.dangerCooldown` is not notified again.
    public var dangerNotified: [String: Double] = [:]

    public init() {}

    private enum CodingKeys: String, CodingKey { case configNotified, dangerActive, dangerNotified }

    /// A damaged field starts empty instead of failing the whole state.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        configNotified = ((try? c.decodeIfPresent([String].self, forKey: .configNotified)) ?? nil) ?? []
        dangerActive = (try? c.decodeIfPresent([String].self, forKey: .dangerActive)) ?? nil
        dangerNotified = ((try? c.decodeIfPresent([String: Double].self, forKey: .dangerNotified)) ?? nil) ?? [:]
    }
}

public enum ChangeNotices {
    /// Transition ids remembered against a second notification.
    public static let maxRemembered = 200
    /// Changes listed one by one in a grouped notification (the others are counted).
    public static let maxListed = 5
    /// A danger gone and back within 24 hours is not notified again (a price hovering around the stop).
    public static let dangerCooldown = 86_400_000.0
    public static let disclaimer = "Conseil indicatif : Altim ne passe aucun ordre."
    public static let dangerAdvice = "Altim ne passe aucun ordre : à vous de décider (réduire, sortir ou accepter le risque)."

    // MARK: Configuration changes

    /// One grouped notification for the transitions just found that were never notified (the newest per asset and
    /// mode); nil when there is nothing new. The state remembers their ids.
    public static func configNotice(_ found: [ConfigTransition], state: ChangeNoticeState) -> (notice: LocalNotice?, state: ChangeNoticeState) {
        let known = Set(state.configNotified)
        var seen = Set<String>()
        let fresh = found.enumerated()
            .sorted { $0.element.at != $1.element.at ? $0.element.at > $1.element.at : $0.offset < $1.offset }
            .map(\.element)
            .filter { !known.contains($0.id) && seen.insert(ConfigChanges.key(kind: $0.kind, symbol: $0.symbol, personal: $0.personal)).inserted }
        guard !fresh.isEmpty else { return (nil, state) }
        var s = state
        s.configNotified = Array((s.configNotified + fresh.map(\.id)).suffix(maxRemembered))
        return (configText(fresh), s)
    }

    /// Texts of the notification: one change with its explanation, missing conditions and triggers (the Radar card's
    /// wording), or several, one line each.
    static func configText(_ list: [ConfigTransition]) -> LocalNotice {
        let assets = Set(list.map(\.asset.id))
        let asset = assets.count == 1 ? assets.first : nil
        let id = "altim.config.\(list[0].id)"
        if list.count == 1 {
            let t = list[0]
            var lines: [String] = []
            if !t.changes.isEmpty { lines.append("Pourquoi le signal a changé : \(t.changes.joined(separator: " ; ")).") }
            if !t.missing.isEmpty { lines.append("Conditions manquantes : \(t.missing.joined(separator: " ; ")).") }
            if !t.triggers.isEmpty { lines.append("Ce qui changerait la décision : \(t.triggers.joined(separator: " ; ")).") }
            if lines.isEmpty { lines.append("Niveau \(t.from.levelLabel) → \(t.to.levelLabel).") }
            lines.append(disclaimer)
            return LocalNotice(id: id, title: t.title, body: lines.joined(separator: "\n"), asset: asset)
        }
        var lines = list.prefix(maxListed).map { t in
            "\(t.symbol) : \(t.change)\(t.missing.isEmpty ? "" : " (conditions manquantes : \(t.missing.joined(separator: " ; ")))")"
        }
        if list.count > maxListed { lines.append("+ \(list.count - maxListed) autre(s) : détail sur le Radar.") }
        lines.append(disclaimer)
        return LocalNotice(id: id, title: "🚨 Changements de configuration · \(list.count)", body: lines.joined(separator: "\n"), asset: asset)
    }

    // MARK: Dangerous positions

    /// "line id:code" for each reason of each dangerous line.
    public static func dangerKeys(_ dangers: [Danger]) -> [String] {
        dangers.flatMap { d in d.reasons.map { "\(d.id):\($0.code.rawValue)" } }
    }

    /// One notification for the lines that newly entered a danger state (a reason absent at the last measurement, and
    /// not notified in the last 24 hours), with all the reasons of those lines; nil otherwise.
    /// - `baseline`: danger keys already known when the state was never measured (the last measurement shown on
    ///   Mes avoirs), so that an update does not notify what the user already saw.
    /// - `nearStopUnknown`: line ids whose daily candles could not be read: their "near the stop" state is unknown and
    ///   kept as it was (a failed download neither ends nor starts a danger).
    public static func dangerNotice(_ dangers: [Danger], state: ChangeNoticeState, now: Double, baseline: [String] = [],
                                    nearStopUnknown: Set<String> = []) -> (notice: LocalNotice?, state: ChangeNoticeState) {
        let previous = state.dangerActive ?? baseline
        let known = Set(previous)
        var active = dangerKeys(dangers)
        let kept = previous.filter { k in
            k.hasSuffix(":\(DangerCode.nearStop.rawValue)") && nearStopUnknown.contains(String(k.dropLast(DangerCode.nearStop.rawValue.count + 1)))
        }
        for k in kept where !active.contains(k) { active.append(k) }
        let fresh = Set(dangerKeys(dangers).filter { k in !known.contains(k) && state.dangerNotified[k].map { now - $0 >= dangerCooldown } ?? true })
        var s = state
        s.dangerActive = active
        s.dangerNotified = s.dangerNotified.filter { now - $0.value < dangerCooldown }
        for k in fresh { s.dangerNotified[k] = now }
        let lines = dangers.filter { d in d.reasons.contains { fresh.contains("\(d.id):\($0.code.rawValue)") } }
        guard !lines.isEmpty else { return (nil, s) }
        return (dangerText(lines, now: now), s)
    }

    /// Label of a danger without any figure.
    static func reasonLabel(_ code: DangerCode) -> String {
        switch code {
        case .stopBroken: return "stop cassé"
        case .nearStop: return "prix à moins d'un ATR du stop"
        case .lossOverRisk: return "perte au-delà de votre risque par idée"
        }
    }

    static let dangerDetail = "Montants et niveaux dans Mes avoirs."

    /// "⚠ Position devenue dangereuse : BTC" (the Mes avoirs and Radar notices' wording), each line's reasons.
    static func dangerText(_ lines: [Danger], now: Double) -> LocalNotice {
        var seen = Set<String>()
        let symbols = lines.map(\.symbol).filter { seen.insert($0).inserted }
        let assets = Set(lines.map { "\($0.kind.rawValue):\($0.symbol)" })
        let title = "⚠ \(lines.count > 1 ? "Positions devenues dangereuses" : "Position devenue dangereuse") : \(symbols.joined(separator: ", "))"
        // Reasons as fixed labels, never the amounts (loss in $, share of the wealth, stop level): a notification
        // can show on the lock screen or a watch; the figures stay in Mes avoirs.
        let body = lines.map { line in
            "\(line.symbol) : \(line.reasons.map { reasonLabel($0.code) }.joined(separator: " ; "))."
        } + [dangerDetail, dangerAdvice]
        return LocalNotice(id: "altim.danger.\(Int(now))", title: title, body: body.joined(separator: "\n"), asset: assets.count == 1 ? assets.first : nil)
    }
}
