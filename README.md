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
- **Efecto cristal nativo y tinte personalizado:** integración con el desenfoque (*blur*) de KWin / `ext-background-effect` en Wayland, modo Claro/Oscuro automático y soporte de tinte personalizado tanto en el dock como en los *popovers* de carpetas.
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
- **Carpetas en pila (*Folder Stacks*):** vistas en **Abanico (Fan)**, **Cuadrícula (Grid)** y **Lista (List)** aceleradas por GPU en las 4 posiciones, con desenfoque nativo, tinte personalizado y liberación automática de memoria en GPU al cerrar.
- **Unidades USB dinámicas (`UDisks2`):** detección y montaje automático de discos externos y memorias USB directamente en el dock.
- **Drag & drop de archivos:** arrastra archivos sobre íconos del dock para abrirlos directamente con la aplicación correspondiente.
- **Internacionalización (9 idiomas):** detección automática de idioma con soporte de respaldo tipográfico para CJK y cirílico.
- **Estabilidad e integración:** bloqueo de instancia única, acción *Salir* en el menú contextual, metadatos AppStream para KDE Discover y compilación nativa en Fedora COPR.
- **Configuración en vivo (`neonpear-dock-settings`):** cambios aplicados al instante sin reiniciar el dock.

---

## Evolución y correcciones (desde Glassdock)

- **Arquitectura inicial (*Glassdock*):** creación del workspace en Rust (`dock-core`, `dock-render`, `dock-wayland`, `dock-daemon`, `dock-settings`) con protocolo `wlr-layer-shell`, renderizado `wgpu`, efecto blur de KWin, magnificación, espaciadores, modo de aplicaciones activas y botón Launchpad.
- **Estabilidad y UX:** correcciones de detección de cursor (*hit-testing*), regiones de entrada en Wayland, animaciones de rebote/atención y sincronización de monitores.
- **Transición a NeonPear Dock (`v0.3.0-beta.3` – `beta.4`):** renombrado completo del proyecto, reemplazo automático del paquete `glassdock` vía RPM (`Obsoletes: glassdock <= 0.3.0`), integración del ícono oficial `NeonPearDock.svg` y corrección del `app_id` de Wayland para mostrar correctamente el ícono en la ventana de configuración.
- **Soporte vertical y escalado (`v0.3.0-beta.5` – `v0.3.0`):** implementación de posiciones Izquierda/Derecha, tooltips y menús contextuales en las 4 direcciones con puente de entrada continuo, auto-escalado proporcional por monitor, pestaña *Acerca de* y suite de pruebas unitarias en `dock-core`.
- **Internacionalización y COPR (`v0.4.0`):** traducción completa a 9 idiomas con detección automática, bloqueo de instancia única, acción *Salir* en el menú contextual, CI en GitHub Actions y empaquetado SRPM con dependencias *vendored* para Fedora COPR.
- **Fan Stack y UDisks2 (`v0.5.0`):** implementación del *Fan Stack* estilo macOS acelerado por GPU en las 4 posiciones, soporte dinámico de unidades USB vía `UDisks2`, optimización del monitor de papelera (`inotify`) y metadatos AppStream para KDE Discover.
- **Vistas Cuadrícula y Lista (`v0.5.5`):** incorporación de los modos **Cuadrícula (Grid)** y **Lista (List)** para carpetas con desenfoque Wayland y tinte personalizado, corrección de alineación y regiones de entrada en docks verticales, cierre automático al abrir el menú contextual y limpieza automática de texturas en GPU.

Para el detalle cronológico completo, consulta [CHANGELOG.md](CHANGELOG.md).

---

## Hoja de ruta

El plan de desarrollo hacia `v1.0.0` (priorizando rendimiento nativo en Wayland y bajo consumo de RAM) se encuentra detallado en [ROADMAP.md](ROADMAP.md). Entre las próximas funciones destacan:

* **Rendimiento (`v0.6`):** carga exclusiva de la tipografía activa del sistema al arranque (sin escaneo completo de fuentes).
* **GUI y Ajustes (`v0.6`):** adopción del tamaño de fuente del sistema, mejor aprovechamiento del espacio horizontal en la ventana de ajustes e ícono SVG adaptativo por luminancia para la flecha en Abanico/Cuadrícula.
* **Interacción y Stacks (`v0.6`):** quitar accesos arrastrándolos fuera del dock, navegación dentro de subcarpetas y ordenamiento de archivos.
* **Multi-monitor (`v0.6`):** selector de monitor o modo "seguir al cursor".
* **Ventanas (`v0.7`):** vista previa de ventanas abiertas al pasar el cursor.

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
