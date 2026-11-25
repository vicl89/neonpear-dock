# NeonPear Dock

[![Release](https://img.shields.io/github/v/release/vicl89/neonpear-dock?include_prereleases&style=flat-square&color=89b4fa)](https://github.com/vicl89/neonpear-dock/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/vicl89/neonpear-dock/ci.yml?branch=master&style=flat-square&label=CI)](https://github.com/vicl89/neonpear-dock/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-a6e3a1?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-Wayland%20%2B%20wgpu-fab387?style=flat-square)](https://www.rust-lang.org/)
[![KDE Plasma 6](https://img.shields.io/badge/KDE%20Plasma-6%20(Wayland)-cba6f7?style=flat-square)](https://kde.org/plasma-desktop/)

**NeonPear Dock** (anteriormente *Glassdock*) es un dock nativo para **Wayland** y **KDE Plasma 6**, escrito en **Rust** y renderizado con **wgpu**, diseñado para ofrecer una experiencia fluida estilo macOS con efecto cristal (*glass blur*), magnificación suave y configuración en caliente.

---

## Vista previa

> **Video de demostración próximamente en YouTube.**
> *(Espacio reservado para incrustar la vista previa en video del dock en acción).*

---

## Características principales

### Apariencia y renderizado
- **Renderizado acelerado por GPU (`wgpu`):** animaciones fluidas de magnificación, rebote (*bounce*) y transiciones de opacidad.
- **Efecto cristal nativo:** integración con el desenfoque (*blur*) de KWin en Wayland.
- **Ícono oficial y lanzadores:** incluye `NeonPearDock.svg`, entrada de menú para el dock y lanzador independiente para la ventana de configuración (`neonpear-dock-settings`).

### Posicionamiento y adaptabilidad
- **Soporte en las 4 posiciones de pantalla:** Abajo, Arriba, Izquierda y Derecha en Wayland.
- **Tooltips y menús contextuales en 4 direcciones:** flechas, animaciones elásticas y puente continuo de región de entrada (*input-region bridging*) para evitar cierres prematuros al mover el cursor desde el ícono al menú.
- **Auto-escalado proporcional:** cuando la cantidad de íconos o el tamaño configurado excede el espacio lógico del monitor activo, el dock reduce proporcionalmente sus métricas en memoria sin alterar `config.toml` y recupera su tamaño original al liberar espacio.

### Productividad e integración con KDE Plasma
- **Magnificación dinámica** configurable (tamaño base, tamaño máximo y espaciado).
- **Ocultado inteligente (*Auto-hide* / *Dodge windows*):** se oculta automáticamente ante ventanas y reacciona al borde del monitor.
- **Gestión de aplicaciones:** indicadores de aplicaciones activas, contadores de notificaciones (*badges*), estado de atención y animación de rebote al lanzar.
- **Espaciadores personalizables** y botón opcional tipo **Launchpad**.
- **Configuración en vivo (`neonpear-dock-settings`):** cambios aplicados al instante sin reiniciar el dock.

---

## Evolución y correcciones (desde Glassdock)

- **Arquitectura inicial (*Glassdock*):** creación del workspace en Rust (`dock-core`, `dock-render`, `dock-wayland`, `dock-daemon`, `dock-settings`) con protocolo `wlr-layer-shell`, renderizado `wgpu`, efecto blur de KWin, magnificación, espaciadores, modo de aplicaciones activas y botón Launchpad.
- **Estabilidad y UX:** correcciones de detección de cursor (*hit-testing*), regiones de entrada en Wayland, animaciones de rebote/atención y sincronización de monitores.
- **Transición a NeonPear Dock (`v0.3.0-beta.3` – `beta.4`):** renombrado completo del proyecto, reemplazo automático del paquete `glassdock` vía RPM (`Obsoletes: glassdock <= 0.3.0`), integración del ícono oficial `NeonPearDock.svg` y corrección del `app_id` de Wayland para mostrar correctamente el ícono en la ventana de configuración.
- **Soporte vertical y escalado (`v0.3.0-beta.5`):** implementación de posiciones Izquierda/Derecha, tooltips y menús contextuales en las 4 direcciones con puente de entrada continuo, y auto-escalado proporcional por monitor.

Para el detalle cronológico completo, consulta [CHANGELOG.md](CHANGELOG.md).

---

## Instalación en Fedora

Descarga el paquete `.rpm` más reciente desde la sección de [Releases](https://github.com/vicl89/neonpear-dock/releases) e instálalo con `dnf`:

```bash
sudo dnf install -y ./neonpear-dock-*.rpm
```

El paquete configura automáticamente el inicio con la sesión (`/etc/xdg/autostart/neonpear-dock.desktop`) y agrega los lanzadores al menú de aplicaciones.

### Compilar desde el código fuente

```bash
git clone https://github.com/vicl89/neonpear-dock.git
cd neonpear-dock
cargo build --release --workspace
```

Los binarios quedarán en `target/release/neonpear-dock` y `target/release/neonpear-dock-settings`.

---

## Autor y soporte

Desarrollado por **FVidriales** (`fernando.vidriales.89@gmail.com`).

Si el proyecto te resulta útil, puedes apoyar su desarrollo aquí:
<a href="https://paypal.me/fvidriales"><img src="https://img.shields.io/badge/Donar-PayPal-00457C?style=flat-square&logo=paypal&logoColor=white" alt="Donar con PayPal"></a>

## Instalación en Fedora (COPR)

[![Copr build status](https://copr.fedorainfracloud.org/coprs/vicl89/neonpear-dock/package/neonpear-dock/status_image/last_build.png)](https://copr.fedorainfracloud.org/coprs/vicl89/neonpear-dock/package/neonpear-dock/)

```bash
sudo dnf copr enable vicl89/neonpear-dock
sudo dnf install neonpear-dock
```

Para actualizar cuando salgan nuevas versiones:

```bash
sudo dnf upgrade neonpear-dock
```
