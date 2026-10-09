# Native handheld UI (design §3.2, plan T3.8 / T4.5 / T4.6).
#
# Build:   cmake -S remote_controller/ui -B build/rc_ui && cmake --build build/rc_ui
# Options: -DRC_UI_WEBENGINE=OFF  build without QtWebEngine (Gate G1 rule 2: no Lichtblick)
#          -DRC_UI_SOUND=OFF      build without QtMultimedia (no alarm tone)
#          -DRC_UI_TESTS=OFF      skip the QtTest unit tests
cmake_minimum_required(VERSION 3.21)
project(rc_ui VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.4 REQUIRED COMPONENTS Core Gui Network Qml Quick)
# One call per optional module: a single call with several components fails as a whole when any
# of them is missing (e.g. no WebEngine would also hide Multimedia and Test).
foreach(_rc_mod WebEngineQuick Multimedia Test)
  find_package(Qt6 QUIET COMPONENTS ${_rc_mod})
endforeach()

# Optional features default to ON when the Qt module is installed.
if(Qt6WebEngineQuick_FOUND)
  set(_rc_we_default ON)
else()
  set(_rc_we_default OFF)
endif()
if(Qt6Multimedia_FOUND)
  set(_rc_snd_default ON)
else()
  set(_rc_snd_default OFF)
endif()
option(RC_UI_WEBENGINE "Host the Lichtblick view through QtWebEngine" ${_rc_we_default})
option(RC_UI_SOUND "Play the loud-alarm tone through QtMultimedia" ${_rc_snd_default})
option(RC_UI_TESTS "Build the QtTest unit tests" ON)

if(RC_UI_WEBENGINE AND NOT Qt6WebEngineQuick_FOUND)
  message(FATAL_ERROR "RC_UI_WEBENGINE=ON but Qt6 WebEngineQuick was not found")
endif()
if(RC_UI_SOUND AND NOT Qt6Multimedia_FOUND)
  message(FATAL_ERROR "RC_UI_SOUND=ON but Qt6 Multimedia was not found")
endif()
message(STATUS "rc_ui: WebEngine=${RC_UI_WEBENGINE} Sound=${RC_UI_SOUND} Tests=${RC_UI_TESTS}")

# --- Backend library (no QML, unit-testable) -------------------------------
add_library(rc_ui_backend STATIC
  src/AppConfig.cpp src/AppConfig.h
  src/IpcFraming.cpp src/IpcFraming.h
  src/IpcClient.cpp src/IpcClient.h
  src/TelemetryModel.cpp src/TelemetryModel.h
  src/AlarmModel.cpp src/AlarmModel.h
  src/ParamModel.cpp src/ParamModel.h
  src/NetStatus.cpp src/NetStatus.h
  src/HostBattery.cpp src/HostBattery.h
)
target_include_directories(rc_ui_backend PUBLIC src)
target_link_libraries(rc_ui_backend PUBLIC Qt6::Core Qt6::Network Qt6::Qml)
target_compile_definitions(rc_ui_backend PUBLIC
  RC_UI_VERSION="${PROJECT_VERSION}"
  RC_UI_HAS_WEBENGINE=$<BOOL:${RC_UI_WEBENGINE}>
  RC_UI_HAS_SOUND=$<BOOL:${RC_UI_SOUND}>)

# --- Application -------------------------------------------------------------
qt_add_executable(rc_ui src/main.cpp)
target_link_libraries(rc_ui PRIVATE rc_ui_backend Qt6::Gui Qt6::Quick)
if(RC_UI_WEBENGINE)
  target_link_libraries(rc_ui PRIVATE Qt6::WebEngineQuick)
endif()
if(RC_UI_SOUND)
  target_link_libraries(rc_ui PRIVATE Qt6::Multimedia)
endif()

set(RC_QML_FILES
  qml/Main.qml
  qml/Theme.qml
  qml/Fmt.qml
  qml/LayoutB.qml
  qml/StatusBar.qml
  qml/Tile.qml
  qml/LinkTile.qml
  qml/BatteryTile.qml
  qml/RobotStatusTile.qml
  qml/ArmTile.qml
  qml/InstrumentColumn.qml
  qml/VideoArea.qml
  qml/Placeholder.qml
  qml/LoudBanner.qml
  qml/ElrsLostScreen.qml
  qml/MenuOverlay.qml
)
set_source_files_properties(qml/Theme.qml qml/Fmt.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
if(RC_UI_WEBENGINE)
  list(APPEND RC_QML_FILES qml/LichtblickWeb.qml)
endif()
if(RC_UI_SOUND)
  list(APPEND RC_QML_FILES qml/AlarmSound.qml)
endif()

if(QT_KNOWN_POLICY_QTP0004)
  qt_policy(SET QTP0004 NEW)
endif()
# Qt 6.4 (Ubuntu noble, the handheld) AOT-compiles the singleton lookups into an endless loop
# (initLoadSingletonLookup spins at 100% CPU at startup): skip the QML compile cache there.
if(Qt6_VERSION VERSION_LESS 6.5)
  set(_rc_qml_opts NO_CACHEGEN)
endif()
qt_add_qml_module(rc_ui
  URI RcUi
  VERSION 1.0
  RESOURCE_PREFIX /qt/qml
  QML_FILES ${RC_QML_FILES}
  RESOURCES resources/alarm.wav
  ${_rc_qml_opts}
)

if(APPLE)
  set_target_properties(rc_ui PROPERTIES MACOSX_BUNDLE FALSE)
endif()

install(TARGETS rc_ui RUNTIME DESTINATION bin)
install(FILES config/rc_ui.json DESTINATION share/rc_ui)

# --- Tests ---------------------------------------------------------------------
if(RC_UI_TESTS AND Qt6Test_FOUND)
  enable_testing()
  qt_add_executable(tst_backend tests/tst_backend.cpp)
  target_link_libraries(tst_backend PRIVATE rc_ui_backend Qt6::Test)
  add_test(NAME tst_backend COMMAND tst_backend)
  set_tests_properties(tst_backend PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endif()
