import Foundation

/// Courtier simulé local (mode démo, sans clé API) : exécution au dernier prix, frais 0,1 %.
public actor PaperBroker: Broker {
    public nonisolated let name = "Démo"
    public nonisolated let environment: BrokerEnvironment = .test
    private var holdings: [String: Decimal]
    private let quoteAsset: String
    private let priceOf: @Sendable (String) async throws -> Double
    private let feeRate: Decimal = Decimal(string: "0.001")!

    public init(startingCash: Decimal = 10_000, quoteAsset: String = "USDT",
                priceOf: @escaping @Sendable (String) async throws -> Double) {
        self.holdings = [quoteAsset: startingCash]
        self.quoteAsset = quoteAsset
        self.priceOf = priceOf
    }

    public func balances() async throws -> [Balance] {
        holdings.filter { $0.value > 0 }.map { Balance(asset: $0.key, free: $0.value, locked: 0) }
            .sorted { $0.asset < $1.asset }
    }

    public func rules(for symbol: String) async throws -> SymbolRules {
        SymbolRules(symbol: symbol, baseAsset: base(of: symbol), quoteAsset: quoteAsset,
                    stepSize: Decimal(string: "0.00001")!, minQuantity: Decimal(string: "0.00001")!,
                    maxQuantity: 0, tickSize: Decimal(string: "0.01")!, minNotional: 5, isTradable: true)
    }

    public func test(_ order: OrderRequest) async throws {
        let price = try await priceOf(order.symbol).decimal
        let rules = try await rules(for: order.symbol)
        var issues = rules.issues(for: order, referencePrice: order.limitPrice ?? price)
        let base = rules.baseAsset
        if order.side == .buy, order.quantity * price * (1 + feeRate) > holdings[quoteAsset, default: 0] {
            issues.append("Solde \(quoteAsset) insuffisant.")
        }
        if order.side == .sell, order.quantity > holdings[base, default: 0] {
            issues.append("Quantité de \(base) insuffisante.")
        }
        if !issues.isEmpty { throw BrokerError.validation(issues) }
    }

    public func place(_ order: OrderRequest) async throws -> OrderResult {
        try await test(order)
        let price = try await priceOf(order.symbol).decimal
        let base = base(of: order.symbol)
        let notional = order.quantity * price
        if order.side == .buy {
            holdings[quoteAsset, default: 0] -= notional * (1 + feeRate)
            holdings[base, default: 0] += order.quantity
        } else {
            holdings[base, default: 0] -= order.quantity
            holdings[quoteAsset, default: 0] += notional * (1 - feeRate)
        }
        return OrderResult(orderId: "DEMO-" + UUID().uuidString.prefix(8), symbol: order.symbol, side: order.side,
                           status: "FILLED", executedQuantity: order.quantity, averagePrice: price,
                           protectionOrderId: nil, isSimulated: true,
                           notes: ["Mode démo : aucun argent réel engagé. Stop/objectif non simulés."])
    }

    private func base(of symbol: String) -> String {
        symbol.hasSuffix(quoteAsset) ? String(symbol.dropLast(quoteAsset.count)) : symbol
    }
}
