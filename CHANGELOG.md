# Changelog

Historial de cambios y evolución de **NeonPear Dock** (anteriormente *Glassdock*).

## [0.5.5] - 2026-11-25
### Añadido
- Modos de vista **Cuadrícula (Grid)** y **Lista (List)** para carpetas fijadas en el dock, además de Abanico y Automático.
- Desenfoque nativo de Wayland (KWin / `ext-background-effect`) y adopción del tinte personalizado (`GlassTheme::Custom`) en los *popovers* de carpetas.

### Corregido y mejorado
- Alineación del dock vertical (Izquierda/Derecha) y cobertura completa de `input_region` con puente continuo hacia la píldora para evitar cierres prematuros.
- Cierre automático del *popover* de carpeta al hacer clic derecho para abrir el menú contextual.
- Liberación automática de texturas y buffers en GPU al cerrar los *stacks*.

## [0.5.0] - 2026-11-25
### Añadido
- **Fan Stack estilo macOS acelerado por GPU (`wgpu`)**: carpetas fijadas en el dock que se despliegan en abanico a 60 fps con animación *ease-out*, cápsulas de texto rotadas, tamaño dinámico ligado a `icon_size`, soporte en las 4 posiciones y traducción en los 9 idiomas soportados (`i18n`).
- Soporte dinámico de **unidades USB montadas** vía `UDisks2` con menú contextual para abrir y expulsar.
- Metadatos **AppStream** (`io.github.vicl89.neonpear-dock.metainfo.xml`) para integración con KDE Discover y flujo de GitHub Actions para adjuntar paquetes RPM a los releases.

### Corregido y mejorado
- Preservación de `pinned_folders` al guardar cambios desde `neonpear-dock-settings`.
- Reducción de uso de CPU en el monitor de papelera ignorando eventos `inotify` no mutantes.

## [0.4.0] - 2026-11-24
### Añadido
- Soporte multiidioma (**i18n**) en 9 idiomas: Español (`es`), Inglés (`en`), Francés (`fr`), Italiano (`it`), Portugués (`pt`), Chino (`zh`), Japonés (`ja`), Coreano (`ko`) y Ruso (`ru`), con detección automática del *locale* (`LC_ALL` / `LC_MESSAGES` / `LANG`), selector manual en `neonpear-dock-settings` y respaldo tipográfico CJK/cirílico vía `fc-match`.
- Bloqueo de instancia única y acción *Salir de NeonPear Dock* en el menú contextual.
- Empaquetado SRPM con crates *vendored* para Fedora COPR y pipeline CI en GitHub Actions (`cargo test --workspace` y `cargo clippy`) con badge de estado en `README.md`.
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
