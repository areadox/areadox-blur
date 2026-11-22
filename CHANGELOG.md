# Changelog

## 0.1.0 – 2026-11-22

Erste Veröffentlichung.

- Algorithmen: Gauß, Box (1–5 Durchgänge), Dual Kawase, Verpixeln (Quadrat/Sechseck)
- Typen: Fläche, Richtung, Zoom (mit scharfem Zentrum), Tilt-Shift
- Masken: Zuschneiden, Rechteck, Kreis, Quelle, Bild – weiche Kante, Umkehren, Kanalwahl
- Hintergrund-Compositing: gemeinsam weichzeichnen oder Milchglas
- Einheitlicher Radius für alle Algorithmen (Standardabweichung = Radius / 2)
- OBS-33-Plugin-Layout mit `manifest.json`, Oberfläche auf Deutsch und Englisch
- Windows-Installer, ZIP für Windows, Pakete für Linux und macOS

### Verbesserungen nach dem Code-Review
- Bild- und Helligkeitsmasken werden wahrnehmungsgetreu ausgewertet (50 % Grau = 50 % Weichzeichnung)
- Schlägt ein GPU-Zwischenschritt fehl, wird die Quelle unverändert angezeigt statt kurz zu verschwinden
- Nicht mehr benötigte Zwischenbilder werden nach ca. 2 Sekunden freigegeben (weniger VRAM)
- Zoom-Weichzeichner mit doppelter Abtastung bei großen Radien (keine Stufen mehr)
- Versionsnummer von Cargo und `manifest.json` wird per Test abgeglichen
