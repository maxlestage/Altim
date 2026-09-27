export function Footer() {
  return (
    <footer className="footer">
      <p className="risk">
        <b>Avertissement sur les risques.</b> Le trading de crypto-actifs et d'instruments financiers comporte un risque
        élevé de perte en capital. Les performances passées ne préjugent pas des performances futures. Altim ne fournit
        pas de conseil en investissement personnalisé et n'est pas un prestataire de services d'investissement.
      </p>
      <p className="muted">© {new Date().getFullYear()} Altim · Fait avec Swift, React & Bun.</p>
    </footer>
  );
}
