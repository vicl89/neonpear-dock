# Hoja de Ruta — NeonPear Dock

Plan de desarrollo desde `v0.6.0` hacia `v1.0.0`, priorizando rendimiento nativo en Wayland y bajo consumo de recursos.

## Próximas funciones

| Categoría | Función | Prioridad / Complejidad | Versión objetivo |
| :--- | :--- | :--- | :--- |
| **Rendimiento** | Cargar únicamente la tipografía activa del sistema al arranque (evitar escaneo completo de fuentes) y evaluar recarga dinámica al cambiar la fuente del sistema | Alta / Media | `v0.6` |
| **GUI / Ajustes** | Adoptar el tamaño de fuente por defecto del sistema en la ventana de ajustes y rediseñar su disposición para aprovechar mejor el espacio horizontal | Alta / Baja | `v0.6` |
| **GUI / Stacks** | Ícono SVG dedicado para la flecha en modos Abanico y Cuadrícula (variante clara/oscura y selección adaptativa por luminancia en modo personalizado) | Media / Baja | `v0.6` |
| **Interacción** | Quitar accesos fijados arrastrándolos fuera del dock (*drag & drop* hacia afuera) | Alta / Baja | `v0.6` |
| **Carpetas / Stacks** | Navegación dentro de subcarpetas (clic en `▸` en modo Lista/Grid) + botón "Atrás" | Alta / Media | `v0.6` |
| **Carpetas / Stacks** | Ordenar contenido (por nombre, fecha de modificación o tipo) desde clic derecho | Media / Baja | `v0.6` |
| **Multi-monitor** | Selector de monitor específico o modo "seguir al cursor" en configuraciones multi-pantalla | Alta / Media | `v0.6` |
| **Ventanas y Flujo** | Vista previa de ventanas al pasar el cursor (miniaturas o lista de ventanas abiertas) | Alta / Alta | `v0.7` |

## Fuera de alcance (Descartado)

Para mantener el dock ligero, nativo y con bajo consumo de RAM, las siguientes funciones quedan descartadas:

* **Distribución en AppImage / Flatpak:** los paquetes nativos (RPM / COPR) garantizan integración directa con el entorno y evitan la sobrecarga de dependencias en contenedor.
* **Applets y widgets complejos dentro del dock** (reloj, clima, monitores de sistema).
* **Soporte para X11:** enfoque exclusivo *Wayland-first*.
* **Animaciones 3D o efectos pesados de partículas.**
* **Perfiles/presets de tema exportables** e **indicadores de app activa personalizables.**
