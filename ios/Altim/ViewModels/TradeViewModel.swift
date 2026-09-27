import Foundation
import Observation
import LocalAuthentication
import AltimCore

/// Préparation, vérification et exécution d'un ordre.
@MainActor
@Observable
final class TradeViewModel {
    enum Phase: Equatable {
        case loading
        case ready
        case executing
        case done
        case failed(String)
    }

    let asset: Asset
    let side: OrderSide
    let signal: Signal?
    let environment: BrokerEnvironment
    private let broker: Broker
    private let settings: AppSettings
    private let services: AppServices

    var orderType: OrderType = .market
    var quantityText = ""
    var limitPriceText = ""
    var protect = true
    var stopText = ""
    var targetText = ""
    var liveConfirmed = false

    private(set) var phase: Phase = .loading
    private(set) var rules: SymbolRules?
    private(set) var balances: [Balance] = []
    private(set) var suggestion: PositionSize?
    private(set) var issues: [String] = []
    private(set) var result: OrderResult?

    /// Dernier cours consolidé (utilisé pour estimer le montant d'un ordre au marché).
    let referencePrice: Double
    /// Fiabilité des données de marché au moment de l'ordre.
    let reliability: ReliabilityLevel?

    init(asset: Asset, side: OrderSide, signal: Signal?, price: Double, reliability: ReliabilityLevel?,
         settings: AppSettings, services: AppServices) {
        self.asset = asset
        self.referencePrice = price
        self.reliability = reliability
        self.side = side
        self.signal = signal
        self.settings = settings
        self.services = services
        self.environment = settings.environment(for: asset)
        self.broker = services.broker(for: asset, settings: settings)
    }

    var brokerName: String { broker.name }
    var isLive: Bool { environment == .live && !settings.demoMode }

    var quoteBalance: Decimal {
        let quote = rules?.quoteAsset ?? asset.quote
        return balances.first { $0.asset == quote }?.free ?? 0
    }

    var baseBalance: Decimal {
        let base = rules?.baseAsset ?? asset.base
        return balances.first { $0.asset == base }?.total ?? 0
    }

    func prepare() async {
        phase = .loading
        do {
            async let rulesTask = broker.rules(for: asset.symbol)
            async let balancesTask = broker.balances()
            rules = try await rulesTask
            balances = try await balancesTask

            guard let rules else { return }
            if side == .buy, let signalPlan = signal?.plan {
                // Le plan est recalé sur le cours actuel en conservant la distance du stop.
                let distance = signalPlan.entry - signalPlan.stopLoss
                let plan = TradePlan(entry: referencePrice, stopLoss: referencePrice - distance,
                                     takeProfit: referencePrice + distance * signalPlan.riskReward)
                stopText = rules.normalizePrice(plan.stopLoss.decimal).plainValue
                targetText = rules.normalizePrice(plan.takeProfit.decimal).plainValue
                suggestion = RiskManager(settings: settings.risk).positionSize(equity: quoteBalance.doubleValue, plan: plan)
                if let s = suggestion {
                    quantityText = rules.normalizeQuantity(s.quantity.decimal).plainValue
                }
            } else if side == .sell {
                quantityText = rules.normalizeQuantity(baseBalance).plainValue
                protect = false
            }
            limitPriceText = rules.normalizePrice(referencePrice.decimal).plainValue
            phase = .ready
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    /// Construit l'ordre et vérifie toutes les règles locales (marché + risque).
    func buildOrder() -> OrderRequest? {
        issues = []
        guard let rules else { issues = ["Règles du marché non chargées."]; return nil }
        guard let rawQty = Self.decimal(quantityText), rawQty > 0 else { issues = ["Quantité invalide."]; return nil }
        let quantity = rules.normalizeQuantity(rawQty)
        let limit = orderType == .limit ? Self.decimal(limitPriceText) : nil
        let stop = side == .buy && protect ? Self.decimal(stopText) : nil
        let target = side == .buy && protect ? Self.decimal(targetText) : nil
        if orderType == .limit && limit == nil { issues.append("Prix limite invalide.") }
        if side == .buy && protect && (stop == nil || target == nil) { issues.append("Stop et objectif requis.") }

        let order = OrderRequest(symbol: asset.symbol, side: side, type: orderType, quantity: quantity,
                                 limitPrice: limit, stopLoss: stop, takeProfit: target)
        let price = limit ?? referencePrice.decimal
        issues += rules.issues(for: order, referencePrice: price)

        let notional = (quantity * price).doubleValue
        if side == .buy {
            var plan: TradePlan?
            if let stop, let target {
                plan = TradePlan(entry: price.doubleValue, stopLoss: stop.doubleValue, takeProfit: target.doubleValue)
            }
            let pnl = services.journal.realizedPnLToday(environment: environment, simulated: settings.demoMode)
            var riskIssues = RiskManager(settings: settings.risk)
                .preTradeIssues(side: .buy, notional: notional, equity: quoteBalance.doubleValue,
                                plan: plan, realizedPnLToday: pnl)
            // Sans protection, l'utilisateur assume le risque : on n'exige plus de stop, le reste s'applique.
            if !protect { riskIssues.removeAll { $0 == "Aucun stop défini : achat refusé." } }
            issues += riskIssues
            if (notional * 1.002).decimal > quoteBalance {
                issues.append("Solde \(rules.quoteAsset) insuffisant (\(Format.quantity(quoteBalance))).")
            }
        } else if quantity > baseBalance {
            issues.append("Vous ne détenez que \(Format.quantity(baseBalance)) \(rules.baseAsset).")
        }
        if side == .buy && reliability == .low {
            issues.append("Données de marché non fiables (sources absentes ou en désaccord) : achat bloqué.")
        }
        if isLive && !liveConfirmed { issues.append("Cochez la confirmation « argent réel ».") }
        return issues.isEmpty ? order : nil
    }

    func execute() async {
        guard let order = buildOrder() else { return }
        phase = .executing
        do {
            // Price anti-error check: the broker must quote the same price as independent sources.
            let broker = self.broker
            let asset = self.asset
            let market = services.market
            async let brokerPriceTask = broker.lastPrice(for: asset.symbol)
            async let consensusTask = market.quote(for: asset)
            let brokerPrice = try await brokerPriceTask.doubleValue
            let consensus = try? await consensusTask
            if let issue = PriceGuard.issue(brokerPrice: brokerPrice, consensusPrice: consensus?.price,
                                            tolerancePercent: asset.assetClass == .crypto ? 1 : 1.5) {
                throw BrokerError.validation([issue])
            }
            try await authenticate()
            try await broker.test(order)
            let result = try await broker.place(order)
            services.journal.record(result, fallbackPrice: referencePrice, environment: environment)
            self.result = result
            phase = .done
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    func reset() { phase = .ready }

    private func authenticate() async throws {
        let context = LAContext()
        var error: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &error) else {
            // Pas de code/biométrie configuré : on refuse l'argent réel.
            if isLive { throw error ?? LAError(.passcodeNotSet) }
            return
        }
        let reason = "\(side.label) de \(quantityText) \(asset.base) · \(isLive ? "ARGENT RÉEL" : "test")"
        _ = try await context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)
    }

    static func decimal(_ text: String) -> Decimal? {
        let cleaned = text.replacingOccurrences(of: " ", with: "").replacingOccurrences(of: "\u{202F}", with: "")
            .replacingOccurrences(of: "\u{00A0}", with: "").replacingOccurrences(of: ",", with: ".")
        return Decimal(string: cleaned, locale: Locale(identifier: "en_US_POSIX"))
    }
}

extension Decimal {
    var doubleValue: Double { NSDecimalNumber(decimal: self).doubleValue }
    var plainValue: String { NSDecimalNumber(decimal: self).description(withLocale: Locale(identifier: "en_US_POSIX")) }
}
