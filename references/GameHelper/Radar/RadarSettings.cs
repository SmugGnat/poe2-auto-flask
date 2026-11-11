// <copyright file="RadarSettings.cs" company="PlaceholderCompany">
// Copyright (c) PlaceholderCompany. All rights reserved.
// </copyright>

namespace Radar
{
    using System.Collections.Generic;
    using System.IO;
    using System.Numerics;
    using GameHelper.Plugin;
    using ImGuiNET;
    using Newtonsoft.Json;
    using GameHelper.Utils;

    /// <summary>
    /// <see cref="Radar"/> plugin settings class.
    /// </summary>
    public sealed class RadarSettings : IPSettings
    {
        private static readonly Vector2 IconSize = new(64, 64);
        private static int poiMonsterGroupNumber = 0;

        /// <summary>
        /// Multipler to apply to the Large Map icons
        /// so they display correctly on the screen.
        /// </summary>
        public float LargeMapScaleMultiplier = 0.1738f;

        /// <summary>
        /// Do not draw the Radar plugin stuff when game is in the background.
        /// </summary>
        public bool DrawWhenForeground = true;

        /// <summary>
        /// Do not draw the Radar plugin stuff when user is in hideout/town.
        /// </summary>
        public bool DrawWhenNotInHideoutOrTown = true;

        /// <summary>
        /// Hides all the entities that are outside the network bubble.
        /// </summary>
        public bool HideOutsideNetworkBubble = false;

        /// <summary>
        /// Gets a value indicating whether user wants to modify large map culling window or not.
        /// </summary>
        public bool ModifyCullWindow = false;

        /// <summary>
        /// Gets a value indicating whether user wants culling window
        /// to cover the full game or not.
        /// </summary>
        public bool MakeCullWindowFullScreen = true;

        /// <summary>
        /// Gets a value indicating whether to draw the map in culling window or not.
        /// </summary>
        public bool DrawMapInCull = true;

        /// <summary>
        /// Gets a value indicating whether to draw the POI in culling window or not.
        /// </summary>
        public bool DrawPOIInCull = true;

        /// <summary>
        /// Gets a value indicating whether user wants to draw walkable map or not.
        /// </summary>
        public bool DrawWalkableMap = true;

        /// <summary>
        /// Gets a value indicating what color to use for drawing walkable map.
        /// </summary>
        public Vector4 WalkableMapColor = new Vector4(150f) / 255f;

        /// <summary>
        /// Gets the position of the cull window that the user wants.
        /// </summary>
        public Vector2 CullWindowPos = Vector2.Zero;

        /// <summary>
        /// Get the size of the cull window that the user wants.
        /// </summary>
        public Vector2 CullWindowSize = Vector2.Zero;

        /// <summary>
        /// Gets a value indicating wether user wants to show Player icon or names.
        /// </summary>
        public bool ShowPlayersNames = false;

        /// <summary>
        /// Gets a value indicating what is the maximum frequency a POI should have
        /// </summary>
        public int POIFrequencyFilter = 0;

        /// <summary>
        /// Gets a value indicating wether user want to show important tgt names or not.
        /// </summary>
        public bool ShowImportantPOI = true;

        /// <summary>
        /// Gets a value indicating what color to use for drawing the POI.
        /// </summary>
        public Vector4 POIColor = new(1f, 0.5f, 0.5f, 1f);

        /// <summary>
        /// Gets a value indicating wether user want to draw a background when drawing the POI.
        /// </summary>
        public bool EnablePOIBackground = true;

        /// <summary>
        /// Gets the Tgts and their expected clusters per area/zone/map.
        /// </summary>
        [JsonIgnore]
        public Dictionary<string, Dictionary<string, string>> ImportantTgts = new();

        /// <summary>
        /// Icons to display on the map. Base game includes normal chests, strongboxes, monsters etc.
        /// </summary>
        public Dictionary<string, IconPicker> BaseIcons = new();

        /// <summary>
        /// Icons to display on the map. POIMonsters includes icons for monsters that are in custom category created by user
        /// </summary>
        public Dictionary<int, IconPicker> POIMonsters = new();

        /// <summary>
        /// Icons to display on the map. Azmeri includes icons for The Viridian Wildwood
        /// </summary>
        public Dictionary<string, IconPicker> AzmeriIcons = new();

        /// <summary>
        /// Icons to display on the map. Breach includes breach chests.
        /// </summary>
        public Dictionary<string, IconPicker> BreachIcons = new();

        /// <summary>
        /// Icons to display on the map. Legion includes special legion monster chests.
        /// since they can't be covered by base icons.
        /// </summary>
        public Dictionary<string, IconPicker> LegionIcons = new();

        /// <summary>
        /// Icons to display on the map. Delirium includes the special spawners and bombs that
        /// delirium brings and they can't be convered by base icons.
        /// </summary>
        public Dictionary<string, IconPicker> DeliriumIcons = new();

        /// <summary>
        /// Icons to display on the map. Delirium includes the special spawners and bombs that
        /// delirium brings and they can't be convered by base icons.
        /// </summary>
        public Dictionary<string, IconPicker> ExpeditionIcons = new();

        /// <summary>
        /// Icons to display on the map. Delve includes the special delve chests which can't be
        /// distinguished by using base chests icons.
        /// </summary>
        public Dictionary<string, IconPicker> DelveIcons = new();

        /// <summary>
        /// Icons to display on the map.
        /// </summary>
        public Dictionary<string, IconPicker> AbyssIcons = new();

        /// <summary>
        /// Draws the icons setting via the ImGui widgets.
        /// </summary>
        /// <param name="headingText">Text to display as heading.</param>
        /// <param name="icons">Icons settings to draw.</param>
        /// <param name="helpingText">helping text to display at the top.</param>
        public void DrawIconsSettingToImGui(
            string headingText,
            Dictionary<string, IconPicker> icons,
            string helpingText)
        {
            var isOpened = ImGui.TreeNode($"{headingText}##treeNode");
            if (!string.IsNullOrEmpty(helpingText))
            {
                ImGuiHelper.ToolTip(helpingText);
            }

            if (isOpened)
            {
                ImGui.Columns(2, $"icons columns##{headingText}", false);
                foreach (var icon in icons)
                {
                    ImGui.Text(icon.Key);
                    ImGui.NextColumn();
                    icon.Value.ShowSettingWidget();
                    ImGui.NextColumn();
                }

                ImGui.Columns(1);
                ImGui.TreePop();
            }
        }

        /// <summary>
        ///     draws the POIMonster setting widget.
        /// </summary>
        /// <param name="dllDirectory">directory where the plugin dll is located.</param>
        public void DrawPOIMonsterSettingToImGui(string dllDirectory)
        {
            if (ImGui.TreeNode($"POI Monster"))
            {
                ImGui.Columns(2, $"icons columns##POIMonsterCol", false);
                foreach (var poimonster in this.POIMonsters)
                {
                    ImGui.Text(poimonster.Key  == -1 ? "Default Group" : $"Group {poimonster.Key}");
                    ImGui.NextColumn();
                    poimonster.Value.ShowSettingWidget();
                    ImGui.SameLine();
                    if (poimonster.Key != -1 && ImGui.Button($"Delete##{poimonster.Key}"))
                    {
                        _ = this.POIMonsters.Remove(poimonster.Key);
                    }

                    ImGui.NextColumn();
                }

                ImGui.Columns(1);
                ImGui.Separator();
                ImGui.SetNextItemWidth(ImGui.GetFontSize() * 5);
                if (ImGui.InputInt("POI Monster Group Number", ref poiMonsterGroupNumber) && poiMonsterGroupNumber < 0)
                {
                    poiMonsterGroupNumber = 0;
                }

                ImGui.SameLine();
                if (ImGui.Button("Add##POIMonsterGroupAdd"))
                {
                    this.POIMonsters.TryAdd(poiMonsterGroupNumber, new(Path.Join(dllDirectory, "icons.png"), 12, 44, 30, IconSize));
                }

                ImGui.TreePop();
            }
        }

        /// <summary>
        /// Adds the default icons if the setting file isn't available.
        /// </summary>
        /// <param name="dllDirectory">directory where the plugin dll is located.</param>
        public void AddDefaultIcons(string dllDirectory)
        {
            var basicIconPathName = Path.Join(dllDirectory, "icons.png");
            this.AddDefaultBaseGameIcons(basicIconPathName);
            this.AddDefaultPOIMonsterIcons(basicIconPathName);
            this.AddDefaultAzmeriIcons(basicIconPathName);
            this.AddDefaultBreachIcons(basicIconPathName);
            this.AddDefaultLegionIcons(basicIconPathName);
            this.AddDefaultDeliriumIcons(basicIconPathName);
            this.AddDefaultDelveIcons(basicIconPathName);
            this.AddDefaultExpeditionIcons(basicIconPathName);
            this.AddDefaultAbyssIcons(basicIconPathName);
        }

        private void AddDefaultBaseGameIcons(string iconPathName)
        {
            this.BaseIcons.TryAdd("Self", new IconPicker(iconPathName, 0, 0, 20, IconSize));
            this.BaseIcons.TryAdd("Player", new IconPicker(iconPathName, 2, 0, 20, IconSize));
            this.BaseIcons.TryAdd("Leader", new IconPicker(iconPathName, 3, 1, 20, IconSize));
            this.BaseIcons.TryAdd("NPC", new IconPicker(iconPathName, 3, 0, 30, IconSize));
            this.BaseIcons.TryAdd("Special NPC", new IconPicker(iconPathName, 13, 42, 100, IconSize));
            this.BaseIcons.TryAdd("Strongbox", new IconPicker(iconPathName, 8, 38, 30, IconSize));
            this.BaseIcons.TryAdd("Important Strongboxes", new IconPicker(iconPathName, 6, 41, 35, IconSize));
            this.BaseIcons.TryAdd("Chests With Label", new IconPicker(iconPathName, 1, 13, 20, IconSize));
            this.BaseIcons.TryAdd("Chests Without Label", new IconPicker(iconPathName, 6, 9, 20, IconSize));

            this.BaseIcons.TryAdd("Shrine", new IconPicker(iconPathName, 7, 0, 30, IconSize));

            this.BaseIcons.TryAdd("Friendly", new IconPicker(iconPathName, 1, 0, 20, IconSize));
            this.BaseIcons.TryAdd("Normal Monster", new IconPicker(iconPathName, 0, 14, 20, IconSize));
            this.BaseIcons.TryAdd("Magic Monster", new IconPicker(iconPathName, 6, 3, 20, IconSize));
            this.BaseIcons.TryAdd("Rare Monster", new IconPicker(iconPathName, 5, 4, 20, IconSize));
            this.BaseIcons.TryAdd("Unique Monster", new IconPicker(iconPathName, 5, 14, 30, IconSize));
            this.BaseIcons.TryAdd("Pinnacle Boss Not Attackable", new IconPicker(iconPathName, 5, 15, 30, IconSize));

            this.BaseIcons.TryAdd("Yellow Bestiary Monster", new IconPicker(iconPathName, 6, 2, 35, IconSize));
            this.BaseIcons.TryAdd("Red Bestiary Monster", new IconPicker(iconPathName, 7, 2, 35, IconSize));
            this.BaseIcons.TryAdd("Harbinger Monster", new IconPicker(iconPathName, 1, 36, 30, IconSize));
            this.BaseIcons.TryAdd("Tormented Spirits", new IconPicker(iconPathName, 12, 14, 30, IconSize));
            this.BaseIcons.TryAdd("Betrayal Enemy NPC", new IconPicker(iconPathName, 5, 11, 30, IconSize));
        }

        private void AddDefaultPOIMonsterIcons(string iconPathName)
        {
            this.POIMonsters.TryAdd(-1, new IconPicker(iconPathName, 12, 44, 30, IconSize));
        }

        private void AddDefaultAzmeriIcons(string iconPathName)
        {
            this.AzmeriIcons.TryAdd("Sacrifice Altar", new IconPicker(iconPathName, 8, 40, 32, IconSize));
            this.AzmeriIcons.TryAdd("Flask Refill Well", new IconPicker(iconPathName, 13, 40, 32, IconSize));
            this.AzmeriIcons.TryAdd("Experience Gain Shrine", new IconPicker(iconPathName, 0, 15, 32, IconSize));
            this.AzmeriIcons.TryAdd("Increase Quantity Shrine", new IconPicker(iconPathName, 0, 15, 32, IconSize));
            this.AzmeriIcons.TryAdd("Can Not Be Damage Shrine", new IconPicker(iconPathName, 0, 15, 50, IconSize));

            this.AzmeriIcons.TryAdd("Wisp Convertor", new IconPicker(iconPathName, 12, 11, 40, IconSize));
            this.AzmeriIcons.TryAdd("Light Bomb", new IconPicker(iconPathName, 4, 38, 40, IconSize));
            this.AzmeriIcons.TryAdd("Fuel Refill", new IconPicker(iconPathName, 0, 11, 50, IconSize));

            this.AzmeriIcons.TryAdd("Purple Wisp Sml", new IconPicker(iconPathName, 8, 28, 12, IconSize));
            this.AzmeriIcons.TryAdd("Blue Wisp Sml", new IconPicker(iconPathName, 10, 27, 12, IconSize));
            this.AzmeriIcons.TryAdd("Yellow Wisp Sml", new IconPicker(iconPathName, 7, 27, 12, IconSize));
            this.AzmeriIcons.TryAdd("Purple Wisp Med", new IconPicker(iconPathName, 8, 28, 20, IconSize));
            this.AzmeriIcons.TryAdd("Blue Wisp Med", new IconPicker(iconPathName, 10, 27, 20, IconSize));
            this.AzmeriIcons.TryAdd("Yellow Wisp Med", new IconPicker(iconPathName, 7, 27, 20, IconSize));
            this.AzmeriIcons.TryAdd("Purple Wisp Big", new IconPicker(iconPathName, 8, 28, 34, IconSize));
            this.AzmeriIcons.TryAdd("Blue Wisp Big", new IconPicker(iconPathName, 10, 27, 34, IconSize));
            this.AzmeriIcons.TryAdd("Yellow Wisp Big", new IconPicker(iconPathName, 7, 27, 34, IconSize));

            this.AzmeriIcons.TryAdd("Unique Trader NPC", new IconPicker(iconPathName, 13, 42, 100, IconSize));
            this.AzmeriIcons.TryAdd("Harvest NPC", new IconPicker(iconPathName, 9, 40, 32, IconSize));
        }

        private void AddDefaultAbyssIcons(string iconPathName)
        {
            this.AbyssIcons.TryAdd("Start", new IconPicker(iconPathName, 13, 0, 16, IconSize));
            this.AbyssIcons.TryAdd("Crack", new IconPicker(iconPathName, 0, 1, 0, IconSize));
            this.AbyssIcons.TryAdd("Mid", new IconPicker(iconPathName, 13, 0, 0, IconSize));
            this.AbyssIcons.TryAdd("End", new IconPicker(iconPathName, 13, 0, 0, IconSize));
        }

        private void AddDefaultLegionIcons(string iconPathName)
        {
            this.LegionIcons.TryAdd("Legion Reward Monster/Chest", new IconPicker(iconPathName, 4, 41, 30, IconSize));
            this.LegionIcons.TryAdd("Legion Epic Chest", new IconPicker(iconPathName, 6, 41, 30, IconSize));
        }

        private void AddDefaultBreachIcons(string iconPathName)
        {
            this.BreachIcons.TryAdd("Breach Chest", new IconPicker(iconPathName, 6, 41, 30, IconSize));
        }

        private void AddDefaultDeliriumIcons(string iconPathName)
        {
            this.DeliriumIcons.TryAdd("Delirium Bomb", new IconPicker(iconPathName, 5, 0, 30, IconSize));
            this.DeliriumIcons.TryAdd("Delirium Spawner", new IconPicker(iconPathName, 6, 0, 30, IconSize));
        }

        private void AddDefaultExpeditionIcons(string iconPathName)
        {
            this.ExpeditionIcons.TryAdd("Generic Expedition Chests", new IconPicker(iconPathName, 5, 41, 30, IconSize));
        }

        private void AddDefaultDelveIcons(string iconPathName)
        {
            this.DelveIcons.TryAdd("Blockage OR DelveWall", new IconPicker(iconPathName, 1, 11, 30, IconSize));
            this.DelveIcons.TryAdd("AberrantFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("AethericFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("AethericFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("BloodstainedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("BoundFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("BoundFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("ClothMajorSpider", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("CorrodedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("CorrodedFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveAzuriteChest1_1", new IconPicker(iconPathName, 7, 18, 30, IconSize));
            this.DelveIcons.TryAdd("DelveAzuriteChest1_2", new IconPicker(iconPathName, 7, 18, 30, IconSize));
            this.DelveIcons.TryAdd("DelveAzuriteChest1_3", new IconPicker(iconPathName, 5, 18, 60, IconSize));
            this.DelveIcons.TryAdd("DelveAzuriteChest2_1", new IconPicker(iconPathName, 5, 18, 60, IconSize));
            this.DelveIcons.TryAdd("DelveAzuriteVein1_1", new IconPicker(iconPathName, 7, 18, 30, IconSize));
            this.DelveIcons.TryAdd("DelveAzuriteVein1_2", new IconPicker(iconPathName, 7, 18, 30, IconSize));
            this.DelveIcons.TryAdd("DelveAzuriteVein1_3", new IconPicker(iconPathName, 5, 18, 60, IconSize));
            this.DelveIcons.TryAdd("DelveAzuriteVein2_1", new IconPicker(iconPathName, 5, 18, 60, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmour1", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmour2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmour3", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmour6LinkedUniqueBody", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourAtlas", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourBody2AdditionalSockets", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourCorrupted", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourDivination", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourElder", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourExtraQuality", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourFullyLinkedBody", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourLife", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourMovementSpeed", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourMultipleResists", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourMultipleUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourOfCrafting", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourShaper", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestAssortedFossils", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrency1", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrency2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrency3", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencyDivination", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencyHighShards", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencyMaps", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencyMaps2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencySilverCoins", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencySockets", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencySockets2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencyVaal", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencyWisdomScrolls", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGem1", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGem2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGem3", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGemGCP", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGemHighLevel", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGemHighLevelQuality", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGemHighQuality", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGemLevel4Special", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGeneric1", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGeneric2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGeneric3", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericAdditionalUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericAdditionalUniques", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericAtziriFragment", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericCurrency", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericDelveUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericDivination", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericElderItem", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueAbyss", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueAmbushInvasion", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueAnarchy", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueBestiary", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueBeyond", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueBloodlines", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueBreach", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueDomination", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueHarbinger", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueIncursion", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueLegion", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueNemesis", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueOnslaught", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueRampage", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueTalisman", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueTempest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueTorment", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericLeagueWarbands", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericOffering", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericPaleCourtFragment", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericProphecyItem", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericRandomEnchant", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericRandomEssence", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericShaperItem", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericTrinkets", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMap1", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMap2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMap3", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMapChisels", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMapCorrupted", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMapElderFragment", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMapHorizon", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMapSextants", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMapShaped", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMapShaperFragment", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestMapUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialArmourAspect", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialArmourChaos", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialArmourCold", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialArmourFire", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialArmourLightning", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialArmourMana", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialArmourMinion", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialArmourPhysical", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialGenericChaos", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialGenericCold", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialGenericEssence", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialGenericFire", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialGenericLightning", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialGenericMana", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialGenericMinion", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialGenericPhysical", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsAbyss", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsAbyss2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsAspect", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsChaos", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsCold", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsFire", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsLightning", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsMana", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsMinion", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsPhysical", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialTrinketsTalisman", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialUniqueChaos", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialUniqueCold", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialUniqueFire", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialUniqueLightning", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialUniqueMana", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialUniqueMinion", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialUniquePhysical", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialWeaponChaos", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialWeaponCold", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialWeaponFire", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialWeaponLightning", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestSpecialWeaponPhysical", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinkets1", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinkets2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinkets3", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsAmulet", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsAtlas", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsCorrupted", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsDivination", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsElder", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsEyesOfTheGreatwolf", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsJewel", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsMultipleUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsOfCrafting", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsRing", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsShaper", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeapon1", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeapon2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeapon3", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeapon30QualityUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeapon6LinkedTwoHanded", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponCannotRollAttacker", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponCannotRollCaster", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponCaster", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponCorrupted", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponDivination", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponElder", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponExtraQuality", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponMultipleUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponPhysicalDamage", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponShaper", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveMiningSupplies1_1", new IconPicker(iconPathName, 0, 11, 60, IconSize));
            this.DelveIcons.TryAdd("DelveMiningSupplies1_2", new IconPicker(iconPathName, 0, 11, 60, IconSize));
            this.DelveIcons.TryAdd("DelveMiningSupplies2_1", new IconPicker(iconPathName, 0, 11, 60, IconSize));
            this.DelveIcons.TryAdd("DelveMiningSuppliesDynamite", new IconPicker(iconPathName, 0, 11, 60, IconSize));
            this.DelveIcons.TryAdd("DelveMiningSuppliesDynamite2", new IconPicker(iconPathName, 0, 11, 60, IconSize));
            this.DelveIcons.TryAdd("DelveMiningSuppliesFlares1_1", new IconPicker(iconPathName, 0, 11, 60, IconSize));
            this.DelveIcons.TryAdd("DelveMiningSuppliesFlares1_2", new IconPicker(iconPathName, 0, 11, 60, IconSize));
            this.DelveIcons.TryAdd("DenseFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DynamiteArmour", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DynamiteCurrency", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DynamiteCurrency2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DynamiteCurrency3", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DynamiteGeneric", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DynamiteTrinkets", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DynamiteWeapon", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("EnchantedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("EnchantedFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("EncrustedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("FacetedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("FracturedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("FrigidFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("GildedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("GlyphicFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("HollowFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("JaggedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("LucentFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("MetallicFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("OffPathArmour", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("OffPathCurrency", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("OffPathGeneric", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("OffPathTrinkets", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("OffPathWeapon", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PathArmour", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PathCurrency", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PathGeneric", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PathTrinkets", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PathWeapon", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PerfectFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PerfectFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PrismaticFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PrismaticFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PristineFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("Resonator1", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("Resonator2", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("Resonator3", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("SanctifiedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("ScorchedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("SerratedFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("SerratedFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("ShudderingFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("TangledFossilChest", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("AberrantFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DenseFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("EncrustedFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("FrigidFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("JaggedFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("LucentFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("MetallicFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("PristineFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("ScorchedFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("ShudderingFossilChestDynamite", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmour30QualityUnique", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourConqueror", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourFractured", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestArmourSynthesised", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestCurrencyHigh", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestGenericScarab", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsConqueror", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsFractured", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsJewelCluster", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsJewelCorrupted", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsQuality", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestTrinketsSynthesised", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponConqueror", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponFractured", new IconPicker(iconPathName, 0, 0, 30, IconSize));
            this.DelveIcons.TryAdd("DelveChestWeaponSynthesised", new IconPicker(iconPathName, 0, 0, 30, IconSize));
        }
    }
}
