package com.ozeromail.app

import leakcanary.AppWatcher

/** Debug builds report what still holds [thing] a few seconds after it should be gone. */
fun expectGone(thing: Any, what: String) = AppWatcher.objectWatcher.expectWeaklyReachable(thing, what)
