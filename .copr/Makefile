srpm:
	dnf -y install cargo rpm-build tar xz git
	$(eval VERSION := $(shell awk '/^Version:/ {print $$2}' packaging/neonpear-dock.spec))
	mkdir -p rpmbuild/SOURCES
	tar --exclude='.git' --exclude='target' --exclude='rpmbuild' \
		--transform="s,^\.,neonpear-dock-$(VERSION)," \
		-czf rpmbuild/SOURCES/neonpear-dock-$(VERSION).tar.gz .
	cargo vendor vendor
	tar -cJf rpmbuild/SOURCES/neonpear-dock-$(VERSION)-vendor.tar.xz vendor
	rm -rf vendor
	rpmbuild -bs \
		--define "_topdir $(PWD)/rpmbuild" \
		--define "_srcrpmdir $(outdir)" \
		packaging/neonpear-dock.spec
