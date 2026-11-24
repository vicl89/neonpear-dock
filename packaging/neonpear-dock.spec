Name:           neonpear-dock
Version:        0.4.0
Release:        1%{?dist}
Summary:        Dock Wayland moderno con efecto cristal para KDE Plasma
License:        MIT
URL:            https://github.com/vicl89/neonpear-dock
Packager:       FVidriales <fernando.vidriales.89@gmail.com>
Obsoletes:      glassdock <= 0.3.0

%global debug_package %{nil}

%description
NeonPear Dock es un dock nativo para Wayland y KDE Plasma escrito en Rust con
magnificación suave, efecto blur, espaciadores personalizables, modo de
aplicaciones activas y botón opcional tipo Launchpad.

%install
install -Dm755 %{_sourcedir}/neonpear-dock %{buildroot}%{_bindir}/neonpear-dock
install -Dm755 %{_sourcedir}/neonpear-dock-settings %{buildroot}%{_bindir}/neonpear-dock-settings
install -Dm644 %{_sourcedir}/neonpear-dock.desktop %{buildroot}%{_sysconfdir}/xdg/autostart/neonpear-dock.desktop
install -Dm644 %{_sourcedir}/neonpear-dock.desktop %{buildroot}%{_datadir}/applications/neonpear-dock.desktop
install -Dm644 %{_sourcedir}/neonpear-dock-settings.desktop %{buildroot}%{_datadir}/applications/neonpear-dock-settings.desktop
install -Dm644 %{_sourcedir}/NeonPearDock.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/NeonPearDock.svg

%files
%{_bindir}/neonpear-dock
%{_bindir}/neonpear-dock-settings
%{_sysconfdir}/xdg/autostart/neonpear-dock.desktop
%{_datadir}/applications/neonpear-dock.desktop
%{_datadir}/applications/neonpear-dock-settings.desktop
%{_datadir}/icons/hicolor/scalable/apps/NeonPearDock.svg

%changelog
* Tue Nov 24 2026 FVidriales <fernando.vidriales.89@gmail.com> - 0.3.0-1
- Lanzamiento estable v0.3.0: soporte en 4 posiciones, menús contextuales y tooltips direccionales, auto-escalado proporcional, pestaña Acerca de y tests unitarios en dock-core

* Sun Nov 22 2026 FVidriales <fernando.vidriales.89@gmail.com> - 0.3.0-0.3.beta3
- Renombrado a NeonPear Dock con ícono oficial NeonPearDock.svg y lanzador en el menú
