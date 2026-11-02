package com.ferrowl.app.ui.message

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class FitToScreenTest {
    @Test
    fun addsViewportInsideHead() {
        val out = fitToScreen("<html><head><title>x</title></head><body>hi</body></html>")
        assertTrue(out.startsWith("<html><head><meta name=\"viewport\""))
        assertTrue(out.contains("<title>x</title>"))
    }

    @Test
    fun replacesTheMailsViewport() {
        val html = "<html><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" /></head><body></body></html>"
        val out = fitToScreen(html)
        assertEquals(1, Regex("name=\"viewport\"").findAll(out).count())
        assertTrue(!out.contains("initial-scale"))
    }

    @Test
    fun wrapsFragments() {
        val out = fitToScreen("<p>hello</p>")
        assertTrue(out.startsWith("<!doctype html><html><head>"))
        assertTrue(out.endsWith("<body><p>hello</p></body></html>"))
    }
}
