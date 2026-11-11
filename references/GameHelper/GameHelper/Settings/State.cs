// <copyright file="State.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace GameHelper.Settings
{
    using System.Collections.Generic;
    using System.IO;
    using ClickableTransparentOverlay;
    using ClickableTransparentOverlay.Win32;
    using GameHelper.RemoteEnums;
    using GameHelper.RemoteEnums.Entity;
    using Newtonsoft.Json;

    /// <summary>
    ///     Game Helper Core Settings.
    /// </summary>
    internal class State
    {
        /// <summary>
        ///     Core Setting File Information.
        /// </summary>
        [JsonIgnore]
        public static readonly FileInfo CoreSettingFile = new("configs/core_settings.json");

        /// <summary>
        ///     Plugins metadata File information.
        /// </summary>
        [JsonIgnore]
        public static readonly FileInfo PluginsMetadataFile = new("configs/plugins.json");

        /// <summary>
        ///     Folder containing all the plugins.
        /// </summary>
        [JsonIgnore]
        public static readonly DirectoryInfo PluginsDirectory = new("Plugins");

        /// <summary>
        ///     Gets a value indicating whether user is using this atlas keystone or not.
        /// </summary>
        public bool UsingUnendingNightmareDeliriumKeystone = true;

        /// <summary>
        ///     Gets a value indicating whether user wants to disable entity processing
        ///     when in town/hideout.
        /// </summary>
        public bool DisableEntityProcessingInTownOrHideout = false;

        /// <summary>
        ///     Gets a value indicating the max degree of parallelism while reading entity list
        ///     in every frame. This is added because some users wanted to limit the cpu usage
        ///     GH uses irrespective of the FPS loss.
        /// </summary>
        public int EntityReaderMaxDegreeOfParallelism = -1;

        /// <summary>
        ///     Gets the value indicating whether to disable all kind of counters and misc features
        ///     to improve the GH performance when running fully juiced + 6 man party maps.
        ///     (even 1 FPS improvement counts at this stage since ppl have 50k entities
        ///     in the network bubble)
        /// </summary>
        public bool DisableAllCounters = false;

        /// <summary>
        ///     Gets or sets a value indicating whether to hide
        ///     the performance stats window when game is in background.
        /// </summary>
        public bool HidePerfStatsWhenBg = false;

        /// <summary>
        ///     Gets or sets a value indicating wherther to show
        ///     full performance stats window or minimum one.
        /// </summary>
        public bool MinimumPerfStats = true;

        /// <summary>
        ///     Gets a value indicating whether user wants to hide the overlay on start or not.
        /// </summary>
        public bool HideSettingWindowOnStart = false;

        /// <summary>
        ///     Gets or sets a value indicating whether the overlay is running or not.
        /// </summary>
        [JsonIgnore]
        public bool IsOverlayRunning = true;

        /// <summary>
        ///     Gets a value indicating how much time to wait between key presses.
        /// </summary>
        public int KeyPressTimeout = 80;

        /// <summary>
        ///     Gets a value indicating whether the random part added to <see cref="KeyPressTimeout" /> lies between
        ///     <see cref="KeyPressRandomMinMs" /> and <see cref="KeyPressRandomMaxMs" /> instead of the built-in 0-9 ms.
        /// </summary>
        public bool KeyPressRandomDelay = false;

        /// <summary>
        ///     Gets the smallest random part in milliseconds added to <see cref="KeyPressTimeout" />.
        /// </summary>
        public int KeyPressRandomMinMs = 0;

        /// <summary>
        ///     Gets the largest random part in milliseconds added to <see cref="KeyPressTimeout" />.
        /// </summary>
        public int KeyPressRandomMaxMs = 50;

        /// <summary>
        ///     Gets the font pathname to load in ImGui.
        /// </summary>
        public string FontPathName = @"C:\Windows\Fonts\msyh.ttc";

        /// <summary>
        ///     Gets the font size to load in ImGui.
        /// </summary>
        public int FontSize = 18;

        /// <summary>
        ///     Gets the language that the font supports.
        /// </summary>
        public FontGlyphRangeType FontLanguage = FontGlyphRangeType.ChineseSimplifiedCommon;


        /// <summary>
        ///     Gets the custom glyph range to load from the font texture. This is useful in case
        ///     <see cref="FontGlyphRangeType"/> isn't enough. Set it to empty string to disable this
        ///     feature.
        /// </summary>
        public string FontCustomGlyphRange = string.Empty;

        /// <summary>
        ///     Gets or sets hotKey to show/hide the main menu.
        /// </summary>
        public VK MainMenuHotKey = VK.F12;

        /// <summary>
        ///     Gets or sets a value indicating whether
        ///     to show DataVisualization window or not.
        /// </summary>
        public bool ShowDataVisualization = false;

        /// <summary>
        ///     Gets or sets a value indicating whether
        ///     to show KrangledPassiveDetector window or not.
        /// </summary>
        public bool ShowKrangledPassiveDetector = false;

        /// <summary>
        ///     Gets or sets a value indicating whether
        ///     to show Game Ui Explorer or not.
        /// </summary>
        public bool ShowGameUiExplorer = false;

        /// <summary>
        ///     Gets or sets a value indicating whether to show
        ///     the performance stats or not.
        /// </summary>
        public bool ShowPerfStats = false;

        /// <summary>
        ///     Gets or sets a value indicating what big nearby means to the user.
        /// </summary>
        public (int Meaning, bool IsVisible, bool FollowMouse) OuterCircle = (Meaning: 70, IsVisible: false, FollowMouse: false);

        /// <summary>
        ///     Gets or sets a value indicating what small nearby means to the user.
        /// </summary>
        public (int Meaning, bool IsVisible, bool FollowMouse) InnerCircle = (Meaning: 30, IsVisible: false, FollowMouse: false);

        /// <summary>
        ///     Gets a value indicating whether user wants to load the
        ///     preload-loaded-files in hideout or not.
        /// </summary>
        public bool SkipPreloadedFilesInHideout = true;

        /// <summary>
        ///     Gets a value indicating whether user wants to close the Game Helper when
        ///     the game exit or not.
        /// </summary>
        public bool CloseWhenGameExit = false;

        /// <summary>
        ///     Gets a value indiciating if vsync is enabled or not.
        /// </summary>
        public bool Vsync = true;

        /// <summary>
        ///     Gets a value indicating if the game input is keyboard/mouse OR PS4 controller.
        /// </summary>
        public bool EnableControllerMode = false;

        /// <summary>
        ///     Gets the party leader name.
        /// </summary>
        public string LeaderName = string.Empty;

        /// <summary>
        ///     Gets or sets hotKey to disable/enable all rendering.
        /// </summary>
        public VK DisableAllRenderingKey = VK.F9;

        /// <summary>
        ///     Gets or sets the important NPC Paths.
        /// </summary>
        public List<string> SpecialNPCPaths = new();

        /// <summary>
        ///     Gets or sets the custom categories (and its defination) for monsters
        /// </summary>
        public List<(EntityFilterType filtertype, string filter, Rarity rarity, int group)> PoiMonstersCategories = new();

        /// <summary>
        ///     Gets or sets the custom categories (and its defination) for monsters
        /// </summary>
        public List<(EntityFilterType filtertype, string filter, Rarity rarity, GameStats stat, int group)> PoiMonstersCategories2 = new();

        /// <summary>
        ///     Gets or sets a value indiciating if user wants to process useless entities as well
        /// </summary>
        public bool ProcessAllRenderableEntities = false;

        /// <summary>
        ///     Gets a value indicating if user is running Taiwan client or not.
        /// </summary>
        public bool IsTaiwanClient = false;
    }
}
