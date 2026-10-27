#include "traycontroller.h"

#include "appcore.h"

#include <QAction>
#include <QApplication>
#include <QFont>
#include <QIcon>
#include <QMenu>
#include <QPainter>
#include <QPixmap>
#include <QQuickWindow>
#include <QSystemTrayIcon>

#ifdef HAVE_KGLOBALACCEL
#include <KGlobalAccel>
#endif

namespace {

QIcon buildTrayIcon()
{
    QIcon icon = QIcon::fromTheme(QStringLiteral("applications-multimedia"));
    if (!icon.isNull())
        return icon;

    QPixmap pixmap(64, 64);
    pixmap.fill(Qt::transparent);

    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setBrush(QColor(QStringLiteral("#FB7299")));
    painter.setPen(Qt::NoPen);
    painter.drawRoundedRect(QRectF(4, 4, 56, 56), 14, 14);

    QFont font = painter.font();
    font.setBold(true);
    font.setPixelSize(36);
    painter.setFont(font);
    painter.setPen(Qt::white);
    painter.drawText(pixmap.rect(), Qt::AlignCenter, QStringLiteral("B"));
    painter.end();

    return QIcon(pixmap);
}

} // namespace

TrayController::TrayController(QQuickWindow *window, AppCore *core, QObject *parent)
    : QObject(parent)
    , m_window(window)
    , m_core(core)
{
    setupTray();
    setupGlobalShortcut();

    connect(window, &QWindow::visibleChanged, this, &TrayController::updateMenuText);
    updateMenuText();
}

TrayController::~TrayController()
{
    delete m_menu;
}

void TrayController::setupTray()
{
    if (!QSystemTrayIcon::isSystemTrayAvailable())
        return;

    m_tray = new QSystemTrayIcon(buildTrayIcon(), this);
    m_tray->setToolTip(tr("bilimini · Meta+Shift+B 显示/隐藏"));

    m_menu = new QMenu();
    m_toggleAction = m_menu->addAction(QString());
    connect(m_toggleAction, &QAction::triggered, this, &TrayController::toggle);
    m_menu->addSeparator();
    auto *shortcutHint = m_menu->addAction(tr("快捷键：Meta+Shift+B"));
    shortcutHint->setEnabled(false);
    m_menu->addSeparator();
    m_quitAction = m_menu->addAction(tr("退出"));
    connect(m_quitAction, &QAction::triggered, qApp, &QCoreApplication::quit);

    m_tray->setContextMenu(m_menu);

    connect(m_tray, &QSystemTrayIcon::activated, this,
            [this](QSystemTrayIcon::ActivationReason reason) {
                if (reason == QSystemTrayIcon::Trigger
                    || reason == QSystemTrayIcon::DoubleClick)
                    toggle();
            });

    m_tray->show();
}

void TrayController::setupGlobalShortcut()
{
#ifdef HAVE_KGLOBALACCEL
    m_globalAction = new QAction(this);
    m_globalAction->setObjectName(QStringLiteral("toggle_window"));
    m_globalAction->setText(tr("显示/隐藏 bilimini"));

    const QList<QKeySequence> shortcut{QKeySequence(QStringLiteral("Meta+Shift+B"))};
    KGlobalAccel::self()->setDefaultShortcut(m_globalAction, shortcut);
    KGlobalAccel::self()->setShortcut(m_globalAction, shortcut);

    connect(m_globalAction, &QAction::triggered, this, &TrayController::toggle);
#endif
}

void TrayController::toggle()
{
    if (m_window && m_window->isVisible())
        hideWindow();
    else
        showWindow();
}

void TrayController::showWindow()
{
    if (!m_window)
        return;
    m_window->show();
    m_window->raise();
    m_window->requestActivate();
}

void TrayController::hideWindow()
{
    if (m_window)
        m_window->hide();
}

void TrayController::updateMenuText()
{
    if (!m_toggleAction || !m_window)
        return;
    m_toggleAction->setText(m_window->isVisible() ? tr("隐藏窗口") : tr("显示窗口"));
}
