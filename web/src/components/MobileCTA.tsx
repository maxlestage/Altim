import { useEffect, useState } from "react";

/** Sticky bar at the bottom of the screen on mobile, shown once the hero has been scrolled past. */
export function MobileCTA() {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const onScroll = () => {
      const download = document.getElementById("download");
      const nearEnd = download ? download.getBoundingClientRect().top < window.innerHeight : false;
      setVisible(window.scrollY > window.innerHeight * 0.8 && !nearEnd);
    };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);
  return (
    <div className={visible ? "mobile-cta show" : "mobile-cta"} aria-hidden={!visible}>
      <div>
        <b>Altim</b>
        <small>Signaux crypto & actions</small>
      </div>
      <a href="#download" className="btn btn-small" tabIndex={visible ? 0 : -1}>
        Télécharger
      </a>
    </div>
  );
}
