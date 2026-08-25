.PHONY: package-deb package-rpm package-msix

package-deb: ## Build and verify a Debian x64 package.
	@test "$(HOST_OS)" = linux || { printf 'Debian packaging requires Linux.\n' >&2; exit 1; }
	@cd $(ROOT); \
	version="$$(packaging/linux/resolve-version.sh)"; \
	$(GRADLE) :shared:jvmTest :desktopApp:packageReleaseDeb \
		-Pvnidrop.desktop.rustVariant=release \
		-Pvnidrop.diagnostics.included=false \
		$(GRADLE_RELEASE_FLAGS); \
	mapfile -t packages < <(find desktopApp/build/compose/binaries/main-release/deb -maxdepth 1 -type f -name '*.deb'); \
	(( $${#packages[@]} == 1 )) || { printf 'Expected exactly one Debian package, found %s\n' "$${#packages[@]}" >&2; exit 1; }; \
	output_directory=build/release/linux/deb; \
	output_name="vnidrop_$${version}-1_amd64.deb"; \
	mkdir -p "$$output_directory"; \
	cp "$${packages[0]}" "$$output_directory/$$output_name"; \
	packaging/linux/patch-deb-desktop-entry.sh "$$output_directory/$$output_name"; \
	packaging/linux/verify-package.sh deb "$$version" "$$output_directory/$$output_name"; \
	( cd "$$output_directory" && sha256sum "$$output_name" > "$$output_name.sha256" ); \
	printf 'Package: %s/%s\n' "$$output_directory" "$$output_name"

package-rpm: ## Build and verify an RPM x64 package.
	@test "$(HOST_OS)" = linux || { printf 'RPM packaging requires Linux.\n' >&2; exit 1; }
	@cd $(ROOT); \
	version="$$(packaging/linux/resolve-version.sh)"; \
	$(GRADLE) :desktopApp:packageReleaseRpm \
		-Pvnidrop.desktop.rustVariant=release \
		-Pvnidrop.diagnostics.included=false \
		$(GRADLE_RELEASE_FLAGS); \
	mapfile -t packages < <(find desktopApp/build/compose/binaries/main-release/rpm -maxdepth 1 -type f -name '*.rpm'); \
	(( $${#packages[@]} == 1 )) || { printf 'Expected exactly one RPM package, found %s\n' "$${#packages[@]}" >&2; exit 1; }; \
	output_directory=build/release/linux/rpm; \
	output_name="vnidrop-$${version}-1.x86_64.rpm"; \
	mkdir -p "$$output_directory"; \
	cp "$${packages[0]}" "$$output_directory/$$output_name"; \
	packaging/linux/verify-package.sh rpm "$$version" "$$output_directory/$$output_name"; \
	( cd "$$output_directory" && sha256sum "$$output_name" > "$$output_name.sha256" ); \
	printf 'Package: %s/%s\n' "$$output_directory" "$$output_name"

package-msix: ## Build and verify an unsigned Windows Store MSIX.
	@test "$(HOST_OS)" = windows || { printf 'MSIX packaging requires Windows.\n' >&2; exit 1; }
	cd $(ROOT) && $(GRADLE) :shared:jvmTest :desktopApp:createReleaseDistributable \
		-Pvnidrop.desktop.rustVariant=release \
		-Pvnidrop.diagnostics.included=false \
		$(GRADLE_RELEASE_FLAGS)
	cd $(ROOT) && $(POWERSHELL) -NoProfile -File packaging/windows/build-msix.ps1 \
		-AppImage desktopApp/build/compose/binaries/main-release/app/VniDrop \
		-OutputDirectory build/release/windows
