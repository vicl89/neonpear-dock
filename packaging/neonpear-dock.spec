Name:           neonpear-dock
Version:        0.5.0
Release:        2%{?dist}
Summary:        Dock Wayland moderno con efecto cristal para KDE Plasma
License:        MIT
URL:            https://github.com/vicl89/neonpear-dock
Packager:       FVidriales <fernando.vidriales.89@gmail.com>
Obsoletes:      glassdock <= 0.3.0

Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  wayland-devel
BuildRequires:  libxkbcommon-devel
BuildRequires:  fontconfig-devel
BuildRequires:  mesa-libGL-devel

%global debug_package %{nil}

%description
NeonPear Dock es un dock nativo para Wayland y KDE Plasma escrito en Rust con
magnificación suave, efecto blur, espaciadores personalizables, modo de
aplicaciones activas y botón opcional tipo Launchpad.

%prep
%autosetup -n %{name}-%{version}
tar -xf %{SOURCE1}
mkdir -p .cargo
cat <<'EOF' > .cargo/config.toml
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF

%build
cargo build --release --workspace --offline --locked

%install
install -Dm755 target/release/neonpear-dock %{buildroot}%{_bindir}/neonpear-dock
install -Dm755 target/release/neonpear-dock-settings %{buildroot}%{_bindir}/neonpear-dock-settings
install -Dm644 packaging/neonpear-dock.desktop %{buildroot}%{_sysconfdir}/xdg/autostart/neonpear-dock.desktop
install -Dm644 packaging/neonpear-dock.desktop %{buildroot}%{_datadir}/applications/neonpear-dock.desktop
install -Dm644 packaging/neonpear-dock-settings.desktop %{buildroot}%{_datadir}/applications/neonpear-dock-settings.desktop
install -Dm644 assets/NeonPearDock.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/NeonPearDock.svg
install -Dm644 packaging/io.github.vicl89.neonpear-dock.metainfo.xml %{buildroot}%{_datadir}/metainfo/io.github.vicl89.neonpear-dock.metainfo.xml

%files
%{_bindir}/neonpear-dock
%{_bindir}/neonpear-dock-settings
%{_sysconfdir}/xdg/autostart/neonpear-dock.desktop
%{_datadir}/applications/neonpear-dock.desktop
%{_datadir}/applications/neonpear-dock-settings.desktop
%{_datadir}/icons/hicolor/scalable/apps/NeonPearDock.svg
%{_datadir}/metainfo/io.github.vicl89.neonpear-dock.metainfo.xml

%changelog
* Wed Nov 25 2026 FVidriales <fernando.vidriales.89@gmail.com> - 0.5.0-2
- Incluir io.github.vicl89.neonpear-dock.metainfo.xml en %files del paquete RPM

* Wed Nov 25 2026 FVidriales <fernando.vidriales.89@gmail.com> - 0.5.0-1
- Fan Stack estilo macOS acelerado por GPU en las 4 posiciones del dock
- Soporte dinámico de unidades USB (UDisks2) y metadatos AppStream para KDE Discover
- Traducción completa del Fan Stack en 9 idiomas y preservación de carpetas fijadas

* Tue Nov 24 2026 FVidriales <fernando.vidriales.89@gmail.com> - 0.4.0-2
- Compilación desde código fuente (SRPM) con dependencias empaquetadas para Fedora COPR
