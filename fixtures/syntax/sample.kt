// A sample that exercises the Kotlin grammar's highlight captures.
package org.example.store

import kotlin.math.max

/** A point with two coordinates. */
data class Point(val x: Int, val y: Int) {
    fun scaled(factor: Int): Point = Point(x * factor, y * factor)

    companion object {
        val ORIGIN = Point(0, 0)
    }
}

/* A block comment
   across lines. */
enum class Mode(val flag: String) {
    READ("r"),
    WRITE("w"),
}

interface Store<K : Comparable<K>> {
    suspend fun load(keys: List<K>): Int

    fun isEmpty(): Boolean = false
}

@Suppress("UNCHECKED_CAST")
class Cache<K : Comparable<K>>(private val items: Map<K, String> = emptyMap()) : Store<K> {
    private var count = 0
    var delegate: Any? = null

    override suspend fun load(keys: List<K>): Int {
        val found = mutableListOf<String>()
        for (key in keys) {
            val item = items[key] ?: continue
            if (count < 0x10) {
                found += item
                count++
            }
        }
        val separator = '\n'
        val ratio = 1.5e3
        return when {
            found.isEmpty() -> -1
            else -> max(found.size, 1)
        }
    }
}

fun describe(point: Point?): String = "point ${point?.x ?: 0}, ${true} and $point"

object Registry {
    private val names = listOf("a", "b")

    fun first(): String? = names.firstOrNull { it.isNotBlank() }
}
