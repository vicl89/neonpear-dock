Name:           glassdock
Version:        0.3.0
Release:        0.3.beta3%{?dist}
Summary:        Dock Wayland moderno con efecto cristal para KDE Plasma
License:        MIT
URL:            https://github.com/vicl89/glassdock
Packager:       FVidriales <fernando.vidriales.89@gmail.com>

%global debug_package %{nil}

%description
GlassDock es un dock nativo para Wayland y KDE Plasma escrito en Rust con
magnificación suave, efecto blur, espaciadores personalizables, modo de
aplicaciones activas y botón opcional tipo Launchpad.

%install
install -Dm755 %{_sourcedir}/glassdock %{buildroot}%{_bindir}/glassdock
install -Dm755 %{_sourcedir}/glassdock-settings %{buildroot}%{_bindir}/glassdock-settings
install -Dm644 %{_sourcedir}/glassdock.desktop %{buildroot}%{_sysconfdir}/xdg/autostart/glassdock.desktop
install -Dm644 %{_sourcedir}/glassdock-settings.desktop %{buildroot}%{_datadir}/applications/glassdock-settings.desktop

%files
%{_bindir}/glassdock
%{_bindir}/glassdock-settings
%{_sysconfdir}/xdg/autostart/glassdock.desktop
%{_datadir}/applications/glassdock-settings.desktop

%changelog
* Sun Nov 22 2026 FVidriales <fernando.vidriales.89@gmail.com> - 0.3.0-0.3.beta3
- Beta 3: modo solo aplicaciones activas, guardado atómico, botones Aplicar/Aceptar y botón Launchpad
