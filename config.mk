# Default command configuration. Override local tool paths in the ignored
# config.override.mk or on the command line.

ifeq ($(OS),Windows_NT)
HOST_OS := windows
GRADLE ?= ./gradlew.bat
else
HOST_UNAME := $(shell uname -s)
ifeq ($(HOST_UNAME),Darwin)
HOST_OS := macos
else ifeq ($(HOST_UNAME),Linux)
HOST_OS := linux
else
HOST_OS := unknown
endif
GRADLE ?= ./gradlew
endif

CARGO ?= cargo
NPM ?= npm
BUN ?= bun
XCODEBUILD ?= xcodebuild
XCODEGEN ?= xcodegen
OPEN ?= open
POWERSHELL ?= pwsh

override VERSION := $(shell $(ROOT)/packaging/version/resolve-version.sh product)
APPLE_PROFILE ?= debug
APPLE_CONFIGURATION ?= Debug
APPLE_DESTINATION ?=
APPLE_CODE_SIGNING ?= NO
# Simulator tests need entitlements applied — the core stores its endpoint
# identity in the protected keychain, which an unsigned app cannot reach. An
# ad-hoc signature is enough there: no certificate, team, or profile involved.
# Set to a real identity only if a device destination is ever used.
APPLE_TEST_CODE_SIGN_IDENTITY ?= -
APPLE_DERIVED_DATA ?= $(ROOT)/apple/DerivedData

GRADLE_FLAGS ?= --no-daemon --stacktrace
GRADLE_RELEASE_FLAGS ?= --no-daemon --no-configuration-cache --stacktrace
