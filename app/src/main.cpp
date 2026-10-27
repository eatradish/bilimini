#include <clocale>

#include <QCommandLineParser>
#include <QApplication>
#include <QIcon>
#include <QImage>
#include <QLinearGradient>
#include <QLocalServer>
#include <QLocalSocket>
#include <QPainter>
#include <QQmlApplicationEngine>
#include <QQmlEngine>
#include <QQuickWindow>
#include <QScreen>
#include <QRadialGradient>
#include <QTimer>


#include "appcore.h"
#include "mpvitem.h"
#include "traycontroller.h"

namespace {

// Screenshot-only helper: puts the window on a desktop-looking canvas so the
// grab is readable outside of a compositor.
QImage composeOnBackdrop(const QImage &windowImage)
{
    if (windowImage.isNull())
        return windowImage;

    const QSize canvasSize(qMax(1280, windowImage.width() + 520),
                           qMax(800, windowImage.height() + 260));
    QImage canvas(canvasSize, QImage::Format_ARGB32_Premultiplied);
    canvas.fill(Qt::transparent);

    QPainter painter(&canvas);
    painter.setRenderHint(QPainter::Antialiasing, true);

    QLinearGradient background(0, 0, 0, canvasSize.height());
    background.setColorAt(0.0, QColor(0x20, 0x22, 0x2a));
    background.setColorAt(1.0, QColor(0x0e, 0x0f, 0x12));
    painter.fillRect(canvas.rect(), background);

    QRadialGradient glow(QPointF(canvasSize.width() * 0.80, canvasSize.height() * 0.86),
                         canvasSize.width() * 0.38);
    glow.setColorAt(0.0, QColor(0xfb, 0x72, 0x99, 30));
    glow.setColorAt(1.0, QColor(0xfb, 0x72, 0x99, 0));
    painter.fillRect(canvas.rect(), glow);

    const int margin = 32;
    painter.drawImage(canvasSize.width() - windowImage.width() - margin,
                      canvasSize.height() - windowImage.height() - margin,
                      windowImage);
    painter.end();
    return canvas;
}

} // namespace

int main(int argc, char *argv[])
{
    qInstallMessageHandler([](QtMsgType, const QMessageLogContext &, const QString &msg) {
        fprintf(stderr, "[qt] %s\n", qPrintable(msg));
    });
    // libmpv's render API needs a GL context; force Qt Quick's OpenGL RHI
    // backend unless the caller overrides it (screenshot runs use software).
    if (qEnvironmentVariableIsEmpty("QSG_RHI_BACKEND"))
        qputenv("QSG_RHI_BACKEND", "opengl");
    // The libmpv render API and Qt Quick's threaded render loop deadlock on
    // some drivers; the basic loop keeps rendering on the GUI thread.
    if (qEnvironmentVariableIsEmpty("QSG_RENDER_LOOP"))
        qputenv("QSG_RENDER_LOOP", "basic");

    QApplication app(argc, argv);
    // libmpv parses numbers with the C locale; Qt switches LC_NUMERIC to the
    // system locale, which makes mpv_create() fail.
    setlocale(LC_NUMERIC, "C");
    QCoreApplication::setApplicationName(QStringLiteral("bili-mini"));
    QCoreApplication::setOrganizationName(QStringLiteral("bili-mini"));
    QCoreApplication::setApplicationVersion(QStringLiteral(BILI_VERSION));
    app.setWindowIcon(QIcon::fromTheme(QStringLiteral("applications-multimedia")));

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("bilibili mini player"));
    parser.addHelpOption();
    parser.addVersionOption();

    QCommandLineOption optScreenshot(QStringLiteral("screenshot"),
                                     QStringLiteral("render the window to <path> and exit"),
                                     QStringLiteral("path"));
    QCommandLineOption optDelay(QStringLiteral("delay"),
                                QStringLiteral("screenshot delay in ms"),
                                QStringLiteral("ms"), QStringLiteral("800"));
    QCommandLineOption optMode(QStringLiteral("mode"),
                               QStringLiteral("initial mode: mini|expanded"),
                               QStringLiteral("mode"), QStringLiteral("expanded"));
    QCommandLineOption optView(QStringLiteral("view"),
                               QStringLiteral("initial view: browse|player"),
                               QStringLiteral("view"), QStringLiteral("browse"));
    QCommandLineOption optItem(QStringLiteral("item"),
                               QStringLiteral("preselect a feed item index"),
                               QStringLiteral("n"), QStringLiteral("-1"));
    QCommandLineOption optNoCore(QStringLiteral("no-core"),
                                 QStringLiteral("do not spawn bili-core"));
    QCommandLineOption optBackdrop(QStringLiteral("backdrop"),
                                   QStringLiteral("composite the screenshot on a desktop-like backdrop"));
    QCommandLineOption optSearch(QStringLiteral("search"),
                                 QStringLiteral("open the search tab with this query"),
                                 QStringLiteral("query"));
    QCommandLineOption optSearchType(QStringLiteral("search-type"),
                                     QStringLiteral("initial search type: video|user"),
                                     QStringLiteral("type"));
    QCommandLineOption optAutoplay(QStringLiteral("autoplay"),
                                   QStringLiteral("start playing the first feed item"));
    QCommandLineOption optPlayBvid(QStringLiteral("play-bvid"),
                                   QStringLiteral("play this bvid directly"),
                                   QStringLiteral("bvid"));
    QCommandLineOption optStayExpanded(QStringLiteral("stay-expanded"),
                                       QStringLiteral("do not collapse to mini when playing"));
    QCommandLineOption optBrowseAfterPlay(QStringLiteral("browse-after-play"),
                                          QStringLiteral("switch to the browse view after starting playback"));
    QCommandLineOption optDump(QStringLiteral("dump-window"),
                               QStringLiteral("log window geometry/state after one second"));
    QCommandLineOption optSize(QStringLiteral("size"),
                               QStringLiteral("override the initial window size (WxH)"),
                               QStringLiteral("size"));
    parser.addOptions({optScreenshot, optDelay, optMode, optView, optItem, optNoCore, optBackdrop, optDump, optSize, optSearch, optSearchType, optAutoplay, optPlayBvid, optStayExpanded, optBrowseAfterPlay});
    parser.process(app);

    const bool screenshot = parser.isSet(optScreenshot);

    AppCore core;
    core.setInitialMode(parser.value(optMode));
    core.setInitialView(parser.value(optView));
    core.setInitialItem(parser.value(optItem).toInt());
    core.setInitialSearch(parser.value(optSearch));
    core.setInitialSearchType(parser.value(optSearchType));
    core.setInitialAutoplay(parser.isSet(optAutoplay));
    core.setInitialPlayBvid(parser.value(optPlayBvid));
    core.setStayExpanded(parser.isSet(optStayExpanded));
    core.setBrowseAfterPlay(parser.isSet(optBrowseAfterPlay));
    core.setScreenshotMode(screenshot);
    core.setNoCore(parser.isSet(optNoCore));

    // Single instance: a second launch raises the existing window instead of
    // spawning another player.
    if (!screenshot) {
        const QString key = QStringLiteral("bili-mini-instance");
        QLocalSocket probe;
        probe.connectToServer(key);
        if (probe.waitForConnected(200)) {
            probe.write("show\n");
            probe.flush();
            probe.waitForBytesWritten(300);
            return 0;
        }

        QLocalServer::removeServer(key);
        auto *instanceServer = new QLocalServer(&app);
        if (instanceServer->listen(key)) {
            QObject::connect(instanceServer, &QLocalServer::newConnection, &core,
                             [instanceServer, &core] {
                                 while (QLocalSocket *socket = instanceServer->nextPendingConnection()) {
                                     QObject::connect(socket, &QLocalSocket::readyRead, &core,
                                                      [socket, &core] {
                                                          if (!socket->readAll().isEmpty())
                                                              emit core.showRequested();
                                                      });
                                 }
                             });
        }
    }

    core.startCore();
    qmlRegisterSingletonInstance("BiliMini", 1, 0, "AppCore", &core);
    qmlRegisterType<MpvItem>("BiliMini", 1, 0, "MpvItem");

    QQmlApplicationEngine engine;
    engine.loadFromModule("BiliMini", "Main");
    if (engine.rootObjects().isEmpty()) {
        qWarning("failed to load QML module BiliMini/Main");
        return 1;
    }

    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().constFirst());
    if (!window)
        return 1;

    // The tray + global shortcut keep the app alive while the window is hidden.
    if (!screenshot)
        new TrayController(window, &core, &app);

    if (parser.isSet(optSize)) {
        const QStringList parts = parser.value(optSize).split(QLatin1Char('x'));
        bool okWidth = false;
        bool okHeight = false;
        if (parts.size() == 2) {
            const int w = parts.at(0).toInt(&okWidth);
            const int h = parts.at(1).toInt(&okHeight);
            if (okWidth && okHeight && w > 0 && h > 0)
                window->resize(w, h);
        }
    }

    if (parser.isSet(optDump)) {
        QTimer::singleShot(1000, &app, [window] {
            QScreen *screen = window->screen();
            const QRect avail = screen ? screen->availableGeometry() : QRect();
            qInfo("window: visible=%d exposed=%d winId=0x%lx size=%dx%d pos=%d,%d flags=0x%x screen=%s dpr=%.2f available=(%d,%d %dx%d)",
                  int(window->isVisible()), int(window->isExposed()),
                  static_cast<unsigned long>(window->winId()),
                  window->width(), window->height(),
                  window->x(), window->y(), int(window->flags()),
                  screen ? qPrintable(screen->name()) : "-",
                  screen ? screen->devicePixelRatio() : 0.0,
                  avail.x(), avail.y(), avail.width(), avail.height());
        });
    }

    if (screenshot) {
        const QString path = parser.value(optScreenshot);
        const int delay = qMax(50, parser.value(optDelay).toInt());
        const bool backdrop = parser.isSet(optBackdrop);

        QTimer::singleShot(delay, &app, [&app, window, path, backdrop] {
            QImage image = window->grabWindow();
            if (image.isNull()) {
                qWarning("screenshot: grabWindow returned a null image");
            } else {
                if (backdrop)
                    image = composeOnBackdrop(image);
                if (!image.save(path))
                    qWarning("screenshot: failed to save %s", qPrintable(path));
                else
                    qInfo("screenshot: %s (%dx%d)", qPrintable(path), image.width(), image.height());
            }
            app.quit();
        });
    }

    return app.exec();
}
