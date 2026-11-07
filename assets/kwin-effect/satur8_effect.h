/*
    Satur8 - a single-purpose KWin saturation effect.

    This is the compositor half of the KWin (B1) backend. It redirects windows
    into an offscreen texture and runs one tiny GLSL pass that boosts saturation
    (digital vibrance). It never touches the game process - it only re-colours
    pixels the game has already rendered and handed to the compositor, so it is
    outside the scope of anti-cheat (the same category as a monitor's saturation
    OSD).

    SPDX-License-Identifier: GPL-3.0-or-later
*/

#pragma once

#include <config-kwin.h>
#include <effect/offscreeneffect.h>

#include <QString>
#include <QStringList>
#include <QVector3D>
#include <memory>

// KWin 6.8 changed Effect::drawWindow() to return bool.
#if PROJECT_VERSION_MAJOR > 6 \
    || (PROJECT_VERSION_MAJOR == 6 && PROJECT_VERSION_MINOR >= 8)
#define SATUR8_KWIN_DRAWWINDOW_BOOL 1
#else
#define SATUR8_KWIN_DRAWWINDOW_BOOL 0
#endif

namespace KWin
{
class GLShader;
class EffectWindow;
}

class Satur8Effect : public KWin::OffscreenEffect
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.kde.kwin.Effect.Satur8")

public:
    Satur8Effect();
    ~Satur8Effect() override;

    static bool supported();

    void reconfigure(ReconfigureFlags flags) override;
    int requestedEffectChainPosition() const override
    {
        return 99; // late: act on the final, composited look
    }
    bool isActive() const override;

protected:
#if SATUR8_KWIN_DRAWWINDOW_BOOL
    bool drawWindow(const KWin::RenderTarget &renderTarget,
                    const KWin::RenderViewport &viewport,
                    KWin::EffectWindow *w,
                    int mask,
                    const KWin::Region &deviceRegion,
                    KWin::WindowPaintData &data) override;
#else
    void drawWindow(const KWin::RenderTarget &renderTarget,
                    const KWin::RenderViewport &viewport,
                    KWin::EffectWindow *w,
                    int mask,
                    const KWin::Region &deviceRegion,
                    KWin::WindowPaintData &data) override;
#endif

public Q_SLOTS:
    /// Live saturation control over D-Bus. 1.0 = unchanged, >1 = more vivid,
    /// 0 = greyscale. Clamped to satur8-core's 0..=4 range.
    Q_SCRIPTABLE void setSaturation(double saturation);
    Q_SCRIPTABLE double saturation() const;
    /// Blend in linear light (more correct) instead of gamma-encoded sRGB.
    Q_SCRIPTABLE void setLinearLight(bool enabled);
    Q_SCRIPTABLE bool linearLight() const;
    /// Restrict the effect to one output (empty = every output). Returns false if the named output is not currently connected.
    Q_SCRIPTABLE bool setTargetOutput(const QString &output);
    Q_SCRIPTABLE QString targetOutput() const;
    /// Names of the outputs KWin is currently driving.
    Q_SCRIPTABLE QStringList outputNames() const;

private:
    void refreshWindow(KWin::EffectWindow *w);
    bool shouldAffect(const KWin::EffectWindow *w) const;
    bool ensureShader();

    std::unique_ptr<KWin::GLShader> m_shader;
    double m_saturation = 1.0;
    bool m_linear = false;
    bool m_shaderFailed = false;
    QString m_targetOutput;
};
