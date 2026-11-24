# Hoja de ruta — NeonPear Dock

Plan de evolución para **NeonPear Dock** (nativo para Wayland y KDE Plasma 6).

## v0.3.0 (Estable)
- [x] Arquitectura modular en Rust (`dock-core`, `dock-render`, `dock-wayland`, `dock-daemon`, `dock-settings`).
- [x] Soporte en las 4 posiciones de pantalla (Abajo, Arriba, Izquierda, Derecha) y auto-escalado proporcional.
- [x] Tooltips y menús contextuales en 4 direcciones con puente de región de entrada.
- [x] Reordenamiento directo (*Drag & Drop*) de íconos y espaciadores sobre el dock.
- [x] Cambio rápido entre múltiples ventanas (clic medio y rueda del ratón).
- [x] Temas visuales de cristal (Automático/Plasma, Oscuro, Claro y Personalizado con color de tinte).
- [x] Tests unitarios base en `dock-core`.
- [x] Pestaña **Acerca de** en `neonpear-dock-settings`.
- [x] Empaquetado estable RPM (`Release: 1%{?dist}`) y publicación del release `v0.3.0` en GitHub.
- [ ] Incrustar video de demostración de YouTube en `README.md`.

## v0.4.0 (Internacionalización, CI y calidad — Listo para release)
- [x] Workflow de GitHub Actions para ejecutar `cargo test --workspace` y `cargo clippy` en cada commit.
- [x] Cobertura ampliada de tests unitarios en `dock-core` (magnificación, rebote, auto-ocultado y *hit-testing* en las 4 posiciones).
- [x] **Soporte multiidioma (i18n)** en 9 idiomas: Español (`es`), Inglés (`en`), Francés (`fr`), Italiano (`it`), Portugués (`pt`), Chino (`zh`), Japonés (`ja`), Coreano (`ko`) y Ruso (`ru`).
  - Detección automática del *locale* del sistema y selector manual en `neonpear-dock-settings`.
  - Respaldo automático de fuentes CJK y cirílicas en `dock-settings` y `dock-render`.

## v0.5.0+ (Distribución y ecosistema)
- [ ] Repositorio **Fedora COPR** para instalación y actualizaciones automáticas con `dnf`.
- [ ] Soporte opcional para `wlr-foreign-toplevel-management` (rastreo de ventanas en compositores Wayland como Hyprland o Sway, manteniendo KDE Plasma 6 como entorno principal).

## v1.0.0 (Madurez)
- [ ] Importación y exportación de configuraciones (`config.toml`) desde `neonpear-dock-settings`.
- [ ] Pulido final de UX y estabilidad a largo plazo en KDE Plasma 6.
