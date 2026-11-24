# Hoja de ruta — NeonPear Dock

Plan de evolución para **NeonPear Dock** (nativo para Wayland y KDE Plasma 6).

## v0.3.0 (Estable — Actual)
- [x] Arquitectura modular en Rust (`dock-core`, `dock-render`, `dock-wayland`, `dock-daemon`, `dock-settings`).
- [x] Soporte en las 4 posiciones de pantalla (Abajo, Arriba, Izquierda, Derecha) y auto-escalado proporcional.
- [x] Tooltips y menús contextuales en 4 direcciones con puente de región de entrada.
- [x] Tests unitarios base en `dock-core`.
- [x] Pestaña **Acerca de** en `neonpear-dock-settings`.
- [ ] Empaquetado estable RPM (`Release: 1%{?dist}`) y publicación del release `v0.3.0` en GitHub.
- [ ] Incrustar video de demostración de YouTube en `README.md`.

## v0.3.1 (Mantenimiento y CI)
- [ ] Workflow de GitHub Actions para ejecutar `cargo test --workspace` y `cargo clippy` en cada commit.
- [ ] Ampliar cobertura de tests en `dock-core` (magnificación, rebote y *hit-testing* en las 4 posiciones).
- [ ] Ajustes finos de UX tras el uso diario en KDE Plasma 6.

## v0.4.0 (Internacionalización, interacción y personalización)
- [ ] **Soporte multiidioma (i18n)** en 9 idiomas: Español (`es`), Inglés (`en`), Francés (`fr`), Italiano (`it`), Portugués (`pt`), Chino (`zh`), Japonés (`ja`), Coreano (`ko`) y Ruso (`ru`).
  - Detección automática del *locale* del sistema y selector manual en `neonpear-dock-settings`.
  - Soporte de fuentes CJK y cirílicas en el renderizador de menús y tooltips.
- [ ] Reordenamiento directo (*Drag & Drop*) de íconos y espaciadores sobre el dock.
- [ ] Presets visuales (cristal claro, cristal oscuro, Catppuccin Mocha) e importación/exportación de `config.toml`.
- [ ] Mejoras en el cambio rápido entre múltiples ventanas de una misma aplicación.

## v0.5.0+ (Distribución y ecosistema)
- [ ] Repositorio **Fedora COPR** para instalación y actualizaciones automáticas con `dnf`.
- [ ] Soporte opcional para `wlr-foreign-toplevel-management` (rastreo de ventanas en compositores Wayland como Hyprland o Sway, manteniendo KDE Plasma 6 como entorno principal).
