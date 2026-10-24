package com.danifunker.misterkvms.input

import android.annotation.SuppressLint
import android.content.Context
import android.os.SystemClock
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.ViewConfiguration
import com.danifunker.misterkvms.core.MouseButton
import com.danifunker.misterkvms.model.AppModel
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.min

/**
 * The video, drawn by the core into this view's surface, which is also a
 * relative touchpad (IPHONE.md §5.3):
 *
 * - one finger drag: move the mouse; tap: left click; two-finger tap: right
 *   click; double tap: double click; tap then drag: click, then hold the
 *   left button while dragging
 * - pinch: zoom the picture; two-finger drag: pan it
 * - three-finger swipe left/right: next/previous MiSTer; three-finger tap:
 *   the controls menu
 */
@SuppressLint("ViewConstructor")
class TouchpadView(context: Context, private val model: AppModel) : SurfaceView(context) {
    var onMenu: () -> Unit = {}
    var onSwitch: (Int) -> Unit = {}

    private enum class Mode { NONE, PRESSED, MOVING, DRAGGING, TWO, THREE }

    private val density = resources.displayMetrics.density
    private val slop = ViewConfiguration.get(context).scaledTouchSlop.toFloat()
    private val doubleTapSlop = ViewConfiguration.get(context).scaledDoubleTapSlop.toFloat()
    private val doubleTapTimeout = ViewConfiguration.getDoubleTapTimeout().toLong()
    /** How far three fingers travel sideways to switch MiSTer. */
    private val swipeDistance = 60 * density

    private var mode = Mode.NONE
    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f
    private var lastY = 0f
    private var downTime = 0L
    private var mostFingers = 0

    /** A tap's click, held back to see whether a press follows (then it's
     *  a double click, or click-and-drag). */
    private var pendingTap: Runnable? = null
    private var tapX = 0f
    private var tapY = 0f

    // Two fingers: their spread and middle; three: where the middle started.
    private var lastSpread = 0f
    private var lastMidX = 0f
    private var lastMidY = 0f
    private var startMidX = 0f
    private var startMidY = 0f
    private var multiMoved = false
    private var swiped = false

    init {
        keepScreenOn = true
        holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {}

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                model.surfaceChanged(holder.surface, width, height, density)
            }

            override fun surfaceDestroyed(holder: SurfaceHolder) = model.surfaceDestroyed()
        })
    }

    @SuppressLint("ClickableViewAccessibility") // a touchpad: no single click to perform
    override fun onTouchEvent(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> fingerDown(e)
            MotionEvent.ACTION_POINTER_DOWN -> moreFingers(e)
            MotionEvent.ACTION_MOVE -> moved(e)
            MotionEvent.ACTION_UP -> allUp(e)
            MotionEvent.ACTION_CANCEL -> {
                if (mode == Mode.DRAGGING) model.button(MouseButton.LEFT, false)
                mode = Mode.NONE
            }
        }
        return true
    }

    private fun fingerDown(e: MotionEvent) {
        downX = e.x; downY = e.y
        lastX = e.x; lastY = e.y
        downTime = SystemClock.uptimeMillis()
        mostFingers = 1
        val tap = pendingTap
        if (tap != null) {
            removeCallbacks(tap)
            pendingTap = null
            if (hypot(e.x - tapX, e.y - tapY) < doubleTapSlop) {
                // The tap before this press was held back: click it now, so a
                // double tap is a double click and tap-and-drag is click, then drag.
                model.click(MouseButton.LEFT)
                model.button(MouseButton.LEFT, true)
                mode = Mode.DRAGGING
                return
            }
            tap.run()
        }
        mode = Mode.PRESSED
    }

    private fun moreFingers(e: MotionEvent) {
        if (mode == Mode.DRAGGING) model.button(MouseButton.LEFT, false)
        mostFingers = maxOf(mostFingers, e.pointerCount)
        multiMoved = false
        when (e.pointerCount) {
            2 -> {
                mode = Mode.TWO
                lastSpread = spread(e)
                lastMidX = midX(e); lastMidY = midY(e)
                startMidX = lastMidX; startMidY = lastMidY
            }
            3 -> {
                mode = Mode.THREE
                swiped = false
                startMidX = midX(e); startMidY = midY(e)
            }
        }
    }

    private fun moved(e: MotionEvent) {
        when (mode) {
            Mode.PRESSED -> if (hypot(e.x - downX, e.y - downY) > slop) {
                mode = Mode.MOVING
                movePointer(e)
            }
            Mode.MOVING, Mode.DRAGGING -> movePointer(e)
            Mode.TWO -> if (e.pointerCount >= 2) zoomAndPan(e)
            Mode.THREE -> if (e.pointerCount >= 3 && !swiped) {
                val dx = midX(e) - startMidX
                val dy = midY(e) - startMidY
                if (abs(dx) > swipeDistance && abs(dx) > abs(dy)) {
                    swiped = true
                    onSwitch(if (dx < 0) 1 else -1)
                }
            }
            Mode.NONE -> {}
        }
    }

    private fun allUp(e: MotionEvent) {
        val quick = SystemClock.uptimeMillis() - downTime < 400
        when (mode) {
            Mode.PRESSED -> {
                tapX = e.x; tapY = e.y
                val tap = Runnable {
                    pendingTap = null
                    model.click(MouseButton.LEFT)
                }
                pendingTap = tap
                postDelayed(tap, doubleTapTimeout)
            }
            Mode.DRAGGING -> model.button(MouseButton.LEFT, false)
            Mode.TWO -> if (mostFingers == 2 && !multiMoved && quick) model.click(MouseButton.RIGHT)
            Mode.THREE -> if (mostFingers == 3 && !swiped && quick) onMenu()
            Mode.MOVING, Mode.NONE -> {}
        }
        mode = Mode.NONE
    }

    /** Faster finger movement moves the pointer further, so both small
     *  adjustments and crossing the screen work (in dp, as iOS in points). */
    private fun movePointer(e: MotionEvent) {
        val dx = (e.x - lastX) / density
        val dy = (e.y - lastY) / density
        lastX = e.x; lastY = e.y
        val gain = min(1 + hypot(dx, dy) / 6, 4f)
        model.move((dx * gain).toDouble(), (dy * gain).toDouble())
    }

    /** Pinch zooms, two fingers moving together pan; both at once work. */
    private fun zoomAndPan(e: MotionEvent) {
        val spread = spread(e)
        val mx = midX(e)
        val my = midY(e)
        if (!multiMoved && (abs(spread - lastSpread) > slop || hypot(mx - startMidX, my - startMidY) > slop)) {
            multiMoved = true
        }
        if (multiMoved) {
            if (lastSpread > 0 && spread > 0) model.zoom((spread / lastSpread).toDouble())
            if (width > 0 && height > 0) {
                // Content follows the fingers: dragging right shows what's to the left.
                model.pan((-(mx - lastMidX) / width).toDouble(), (-(my - lastMidY) / height).toDouble())
            }
            lastSpread = spread
            lastMidX = mx; lastMidY = my
        }
    }

    private fun spread(e: MotionEvent) = hypot(e.getX(1) - e.getX(0), e.getY(1) - e.getY(0))
    private fun midX(e: MotionEvent) = (0 until e.pointerCount).sumOf { e.getX(it).toDouble() }.toFloat() / e.pointerCount
    private fun midY(e: MotionEvent) = (0 until e.pointerCount).sumOf { e.getY(it).toDouble() }.toFloat() / e.pointerCount
}
