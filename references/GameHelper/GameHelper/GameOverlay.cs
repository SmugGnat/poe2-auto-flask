// <copyright file="GameOverlay.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace GameHelper
{
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using ClickableTransparentOverlay;
    using Coroutine;
    using CoroutineEvents;
    using GameHelper.Utils;
    using ImGuiNET;
    using Plugin;
    using Settings;
    using Ui;

    /// <inheritdoc />
    public sealed class GameOverlay : Overlay
    {
        /// <summary>
        ///     Initializes a new instance of the <see cref="GameOverlay" /> class.
        /// </summary>
        internal GameOverlay(string windowTitle)
#if DEBUG
            : base(windowTitle, true)
#else
            : base(windowTitle, true, 3840, 2160)
#endif
        {
            CoroutineHandler.Start(this.UpdateOverlayBounds(), priority: int.MaxValue);
            SettingsWindow.InitializeCoroutines();
            PerformanceStats.InitializeCoroutines();
            DataVisualization.InitializeCoroutines();
            GameUiExplorer.InitializeCoroutines();
            OverlayKiller.InitializeCoroutines();
            NearbyVisualization.InitializeCoroutines();
            KrangledPassiveDetector.InitializeCoroutines();
        }

        /// <summary>
        ///     Gets the fonts loaded in the overlay.
        /// </summary>
        public ImFontPtr[] Fonts { get; private set; }

        /// <inheritdoc />
        public override async Task Run()
        {
            Core.Initialize();
            Core.InitializeCororutines();
            this.VSync = Core.GHSettings.Vsync;
            await base.Run();
        }

        /// <inheritdoc />
        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                Core.Dispose();
            }

            base.Dispose(disposing);
        }

        /// <inheritdoc />
        protected override Task PostInitialized()
        {
            if (MiscHelper.TryConvertStringToImGuiGlyphRanges(Core.GHSettings.FontCustomGlyphRange, out var glyphRanges))
            {
                Core.Overlay.ReplaceFont(
                    Core.GHSettings.FontPathName,
                    Core.GHSettings.FontSize,
                    glyphRanges);
            }
            else
            {
                Core.Overlay.ReplaceFont(
                    Core.GHSettings.FontPathName,
                    Core.GHSettings.FontSize,
                    Core.GHSettings.FontLanguage);
            }

            PManager.InitializePlugins();
            return Task.CompletedTask;
        }

        /// <inheritdoc />
        protected override void Render()
        {
            CoroutineHandler.Tick(ImGui.GetIO().DeltaTime);
            CoroutineHandler.RaiseEvent(GameHelperEvents.PerFrameDataUpdate);
            CoroutineHandler.RaiseEvent(GameHelperEvents.PostPerFrameDataUpdate);
            CoroutineHandler.RaiseEvent(GameHelperEvents.OnRender);
            if (!Core.GHSettings.IsOverlayRunning)
            {
                this.Close();
            }
        }

        private IEnumerator<Wait> UpdateOverlayBounds()
        {
            while (true)
            {
                yield return new Wait(GameHelperEvents.OnMoved);
                this.Position = Core.Process.WindowArea.Location;
                this.Size = Core.Process.WindowArea.Size;
            }
        }
    }
}