import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import ArduboyMobile

ApplicationWindow {
    id: window
    width: 420
    height: 760
    minimumWidth: 320
    minimumHeight: 520
    visible: true
    title: qsTr("Arduboy Emulator")
    color: "#0b0d10"

    function keyButton(key) {
        switch (key) {
        case Qt.Key_Up: return 0
        case Qt.Key_Down: return 1
        case Qt.Key_Left: return 2
        case Qt.Key_Right: return 3
        case Qt.Key_Z: return 4
        case Qt.Key_X: return 5
        // Android controllers commonly expose their face buttons as A/B key
        // events; desktop Z/X bindings remain available for keyboard testing.
        case Qt.Key_A: return 4
        case Qt.Key_B: return 5
        default: return -1
        }
    }

    FileDialog {
        id: romDialog
        title: qsTr("Open Arduboy ROM")
        nameFilters: [qsTr("ROM files (*.hex *.arduboy *.elf)")]
        onAccepted: emulator.loadRom(selectedFile)
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            Label { text: window.title; font.bold: true; Layout.fillWidth: true }
            Button { text: qsTr("Open ROM"); onClicked: romDialog.open() }
            Button { text: qsTr("Reset"); enabled: emulator.loaded; onClicked: emulator.reset() }
        }
    }

    MobileEmulator {
        id: emulator
        objectName: "emulator"
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.max(30, parent.height * 0.12)
        width: Math.min(parent.width * 0.82, 512)
        height: width / 2
        focus: true
        Keys.onPressed: (event) => {
            const button = window.keyButton(event.key)
            if (button >= 0) { emulator.setButton(button, true); event.accepted = true }
        }
        Keys.onReleased: (event) => {
            const button = window.keyButton(event.key)
            if (button >= 0) { emulator.setButton(button, false); event.accepted = true }
        }
    }

    Label {
        anchors.top: emulator.bottom
        anchors.topMargin: 16
        anchors.horizontalCenter: parent.horizontalCenter
        width: parent.width - 32
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        color: "#aab5c0"
        text: emulator.status
    }

    Item {
        id: controls
        anchors.top: emulator.bottom
        anchors.topMargin: 74
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 28
        // Keep the two control groups separate even on narrow phones.
        property int buttonSize: Math.min(62, Math.max(42, Math.floor((width - 36) / 5)))
        property int controlGap: buttonSize > 52 ? 8 : 6

        Grid {
            id: dpad
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            columns: 3
            spacing: controls.controlGap
            Repeater {
                model: [
                    { label: "", button: -1 }, { label: "▲", button: 0 }, { label: "", button: -1 },
                    { label: "◀", button: 2 }, { label: "", button: -1 }, { label: "▶", button: 3 },
                    { label: "", button: -1 }, { label: "▼", button: 1 }, { label: "", button: -1 }
                ]
                delegate: Item {
                    width: controls.buttonSize
                    height: controls.buttonSize
                    RoundButton {
                        anchors.fill: parent
                        visible: modelData.button >= 0
                        text: ""
                        onPressedChanged: emulator.setButton(modelData.button, pressed)

                        // Use geometry rather than arrow glyphs. Android can
                        // substitute the left/right Unicode arrows with emoji
                        // while the vertical glyphs remain text, producing
                        // inconsistent-looking D-pad buttons.
                        Canvas {
                            anchors.centerIn: parent
                            width: Math.round(controls.buttonSize * 0.42)
                            height: width
                            onWidthChanged: requestPaint()
                            onHeightChanged: requestPaint()
                            onPaint: {
                                const ctx = getContext("2d")
                                ctx.reset()
                                ctx.fillStyle = "#20252b"
                                ctx.beginPath()
                                if (modelData.button === 0) {
                                    ctx.moveTo(width / 2, 0)
                                    ctx.lineTo(width, height)
                                    ctx.lineTo(0, height)
                                } else if (modelData.button === 1) {
                                    ctx.moveTo(0, 0)
                                    ctx.lineTo(width, 0)
                                    ctx.lineTo(width / 2, height)
                                } else if (modelData.button === 2) {
                                    ctx.moveTo(0, height / 2)
                                    ctx.lineTo(width, 0)
                                    ctx.lineTo(width, height)
                                } else {
                                    ctx.moveTo(width, height / 2)
                                    ctx.lineTo(0, height)
                                    ctx.lineTo(0, 0)
                                }
                                ctx.closePath()
                                ctx.fill()
                            }
                        }
                    }
                }
            }
        }

        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: controls.controlGap * 2
            RoundButton {
                text: "B"
                width: controls.buttonSize
                height: controls.buttonSize
                font.pixelSize: Math.round(controls.buttonSize * 0.38)
                onPressedChanged: emulator.setButton(5, pressed)
            }
            RoundButton {
                text: "A"
                width: controls.buttonSize
                height: controls.buttonSize
                font.pixelSize: Math.round(controls.buttonSize * 0.38)
                onPressedChanged: emulator.setButton(4, pressed)
            }
        }
    }
}
