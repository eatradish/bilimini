cmake_minimum_required(VERSION 3.21)

project(bili-mini VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Quick QuickControls2 Network OpenGL Widgets)
find_package(KF6GlobalAccel QUIET)
find_package(PkgConfig REQUIRED)
pkg_check_modules(MPV REQUIRED IMPORTED_TARGET mpv)

qt_standard_project_setup(REQUIRES 6.5)

qt_add_executable(bili-mini
    src/main.cpp
    src/appcore.cpp
    src/appcore.h
    src/mpvitem.cpp
    src/mpvitem.h
    src/traycontroller.cpp
    src/traycontroller.h
)

set(BILI_QML_FILES
    Main.qml
    Api.qml
    Spinner.qml
    DanmakuLayer.qml
    Theme.qml
    MockFeed.qml
    IconButton.qml
    RailButton.qml
    MockCover.qml
    VideoCard.qml
    VideoRow.qml
    UserRow.qml
    UserView.qml
    CommentRow.qml
    PillTab.qml
    SeekBar.qml
    BiliSearchField.qml
    ResizeHandles.qml
    ControlBar.qml
    MiniView.qml
    BrowseView.qml
    PlayerView.qml
    ExpandedView.qml
)

set(BILI_ICONS
    icons/play.svg
    icons/pause.svg
    icons/prev.svg
    icons/next.svg
    icons/volume.svg
    icons/mute.svg
    icons/chevron-up.svg
    icons/chevron-down.svg
    icons/close.svg
    icons/fullscreen.svg
    icons/search.svg
    icons/home.svg
    icons/history.svg
    icons/settings.svg
    icons/user.svg
    icons/danmaku.svg
    icons/back.svg
)

# The generated qmldir must mark these as singletons, otherwise QML resolves
# them as plain types and every Theme.* reference is undefined at runtime.
set_source_files_properties(Theme.qml MockFeed.qml Api.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

qt_add_qml_module(bili-mini
    URI BiliMini
    VERSION 1.0
    QML_FILES ${BILI_QML_FILES}
    RESOURCES ${BILI_ICONS}
)

target_link_libraries(bili-mini PRIVATE
    Qt6::Core
    Qt6::Gui
    Qt6::Quick
    Qt6::QuickControls2
    Qt6::Network
    Qt6::OpenGL
    Qt6::Widgets
    PkgConfig::MPV
)

target_compile_definitions(bili-mini PRIVATE
    BILI_VERSION="${PROJECT_VERSION}"
)

install(TARGETS bili-mini RUNTIME DESTINATION bin)

if(KF6GlobalAccel_FOUND)
    target_link_libraries(bili-mini PRIVATE KF6::GlobalAccel)
    target_compile_definitions(bili-mini PRIVATE HAVE_KGLOBALACCEL)
else()
    message(WARNING "KF6GlobalAccel not found: global shortcut disabled")
endif()
