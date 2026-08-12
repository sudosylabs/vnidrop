package com.vnidrop.app

import org.junit.Assume.assumeNoException

internal fun skipWhenHostCredentialStoreIsUnavailable(error: Throwable): Nothing {
	assumeNoException("host protected credential store is unavailable", error)
	throw AssertionError("JUnit assumption unexpectedly returned", error)
}
