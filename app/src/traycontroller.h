#pragma once

#include <QObject>

class AppCore;
class QAction;
class QMenu;
class QQuickWindow;
class QSystemTrayIcon;

// System tray icon + global shortcut that toggles the main window.
class TrayController : public QObject
{
    Q_OBJECT

public:
    TrayController(QQuickWindow *window, AppCore *core, QObject *parent = nullptr);
    ~TrayController() override;

    Q_INVOKABLE void toggle();
    Q_INVOKABLE void showWindow();
    Q_INVOKABLE void hideWindow();

private:
    void setupTray();
    void setupGlobalShortcut();
    void updateMenuText();

    QQuickWindow *m_window = nullptr;
    AppCore *m_core = nullptr;
    QSystemTrayIcon *m_tray = nullptr;
    QMenu *m_menu = nullptr;
    QAction *m_toggleAction = nullptr;
    QAction *m_quitAction = nullptr;
    QAction *m_globalAction = nullptr;
};
