# Changelog

Historial de cambios y evolución de **NeonPear Dock** (anteriormente *Glassdock*).

## [0.4.0] - 2026-11-24
### Added
- Soporte multiidioma (**i18n**) en 9 idiomas: Español (`es`), Inglés (`en`), Francés (`fr`), Italiano (`it`), Portugués (`pt`), Chino (`zh`), Japonés (`ja`), Coreano (`ko`) y Ruso (`ru`).
- Detección automática del *locale* del sistema (`LC_ALL` / `LC_MESSAGES` / `LANG`) y selector manual de idioma en `neonpear-dock-settings`.
- Respaldo automático de fuentes CJK y cirílicas vía `fc-match` en `dock-settings` (y soporte nativo en `dock-render`).
- Workflow de GitHub Actions (`CI`) para ejecutar `cargo test --workspace` y `cargo clippy` en cada commit, junto con badge de estado en `README.md`.
- Tests unitarios ampliados en `dock-core` cubriendo *hit-testing* en las 4 posiciones, magnificación, animación de rebote, auto-ocultado e i18n (12 tests en total).

## [0.3.0] - 2026-11-24
### Añadido
- Pestaña **Acerca de** en `neonpear-dock-settings` con versión, licencia MIT, enlaces a GitHub/PayPal y botón para copiar el correo de contacto.
- Suite inicial de tests unitarios en `dock-core` (auto-escalado, geometría en 4 posiciones y menús contextuales).
- Hoja de ruta oficial del proyecto en `ROADMAP.md`.

### Cambiado
- Empaquetado estable RPM (`0.3.0-1`) listo para instalación en Fedora con KDE Plasma 6.

## [0.3.0-beta.5] - 2026-11-23
### Añadido
- Soporte completo para posiciones **Izquierda** y **Derecha** en Wayland.
- **Tooltips** y **menús contextuales** adaptados a las 4 direcciones (Abajo, Arriba, Izquierda, Derecha) con flecha direccional y animación elástica alineada al eje activo.
- **Auto-escalado proporcional:** ajuste automático en memoria de `icon_size`, `max_icon_size`, `icon_spacing` y rellenos cuando el dock supera el 94% del eje lógico del monitor activo, sin modificar `config.toml`.

### Corregido
- Puente continuo de región de entrada (*input-region bridging*) entre los íconos y el menú contextual para evitar cierres prematuros al desplazar el cursor.
- Límite del eje principal del lienzo Wayland al tamaño lógico del monitor para evitar recortes de superficie.

## [0.3.0-beta.4] - 2026-11-22
### Cambiado
- Preparación de empaquetado RPM `0.3.0-0.4.beta4` para Fedora con reemplazo limpio de instalaciones previas de `glassdock`.

## [0.3.0-beta.3] - 2026-11-22
### Cambiado
- Renombrado oficial del proyecto de **Glassdock** a **NeonPear Dock**.
- Integración del ícono oficial `NeonPearDock.svg` y lanzadores `.desktop` actualizados para el dock y el panel de configuración.

### Corregido
- Ajuste del `app_id` de Wayland en `neonpear-dock-settings` para que KDE Plasma muestre el ícono correcto en la barra de tareas y el selector de ventanas.

## [0.1.0 – 0.3.0-beta.2] (Etapa Glassdock)
### Añadido
- Arquitectura modular en Rust (`dock-core`, `dock-render`, `dock-wayland`, `dock-daemon`, `dock-settings`).
- Renderizado nativo en Wayland con `wgpu` y efecto cristal (*blur*) integrado con KWin.
- Magnificación suave de íconos, animación de rebote (*bounce*), indicadores de aplicaciones activas y badges de notificación.
- Espaciadores personalizables, modo de aplicaciones activas y botón opcional tipo Launchpad.
- Modo de ocultado inteligente (*dodge windows* / *auto-hide*) y aplicación gráfica de configuración en caliente.
- Empaquetado inicial en formato `.rpm` para Fedora.
