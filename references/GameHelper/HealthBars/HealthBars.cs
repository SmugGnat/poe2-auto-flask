// <copyright file="HealthBars.cs" company="PlaceholderCompany">
// Copyright (c) PlaceholderCompany. All rights reserved.
// </copyright>

namespace HealthBars
{
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Numerics;
    using Coroutine;
    using GameHelper;
    using GameHelper.CoroutineEvents;
    using GameHelper.Plugin;
    using GameHelper.RemoteEnums;
    using GameHelper.RemoteEnums.Entity;
    using GameHelper.RemoteObjects.Components;
    using GameHelper.RemoteObjects.States.InGameStateObjects;
    using GameHelper.Utils;
    using ImGuiNET;
    using Newtonsoft.Json;

    /// <summary>
    ///     <see cref="HealthBars" /> plugin.
    /// </summary>
    public sealed class HealthBars : PCore<HealthBarsSettings>
    {
        private readonly List<string> textureToValidate = new()
        {
            "full_bar.png",
            "hollow_bar.png"
        };

        private int poiMonsterConfigToDelete = 0;
        private int poiMonsterConfigToAdd = 0;
        private float graduationsThickness = 0f;
        private Vector2 fontSize = Vector2.Zero;

        private string SettingPathname => Path.Join(this.DllDirectory, "config", "settings.txt");

        private string TexturesPath => Path.Join(this.DllDirectory, "Textures");

        private readonly TextureLoader textures = new();

        private readonly Dictionary<uint, Vector2> bPositions = new();

        private ActiveCoroutine onAreaChange = null;

        /// <inheritdoc />
        public override void DrawSettings()
        {
            ImGui.Text("Turn off in game health bars for best result.");
            ImGui.Text("Enable/Disable plugin to reload textures.");
            ImGui.Text($"Total Textures loaded: {this.textures.TotalTexturesLoaded}");
            if (ImGui.CollapsingHeader("Common Configuration"))
            {
                if (ImGui.BeginTable("common_config_table", 2))
                {
                    ImGui.TableNextColumn();
                    ImGui.Checkbox("Draw healthbars in town", ref this.Settings.DrawInTown);
                    ImGui.TableNextColumn();
                    ImGui.Checkbox("Draw healthbars in hideout", ref this.Settings.DrawInHideout);
                    ImGui.TableNextColumn();
                    ImGui.Checkbox("Draw healthbars when game is in background", ref this.Settings.DrawWhenGameInBackground);
                    ImGui.TableNextColumn();
                    ImGui.Checkbox("Interpolate position", ref this.Settings.InterpolatePosition);
                    ImGuiHelper.ToolTip("Enable this if your healthbar is stuttering.");
                    ImGui.TableNextColumn();
                    ImGui.DragInt("Cull Strike (%health)", ref this.Settings.CullingStrikeRange, 1, 0, 100);
                    ImGui.EndTable();
                }
            }

            if (ImGui.CollapsingHeader("Monster Configuration"))
            {
                if (ImGui.BeginTabBar("monster_config"))
                {
                    foreach (var item in this.Settings.Monster)
                    {
                        if (ImGui.BeginTabItem(item.Key))
                        {
                            item.Value.Draw();
                            ImGui.EndTabItem();
                        }
                    }

                    ImGui.EndTabBar();
                }
            }

            if (ImGui.CollapsingHeader("POI Configuration"))
            {
                ImGui.SetNextItemWidth(ImGui.GetFontSize() * 10);
                if(ImGui.InputInt("Group Number##poimonsterconfig", ref this.poiMonsterConfigToAdd) && this.poiMonsterConfigToAdd < 0)
                {
                    this.poiMonsterConfigToAdd = 0;
                }

                ImGui.SameLine();
                if (ImGui.Button("Add"))
                {
                    this.Settings.POIMonster.TryAdd(this.poiMonsterConfigToAdd, new());
                }

                if (ImGui.BeginTabBar("poimonster_config", ImGuiTabBarFlags.AutoSelectNewTabs))
                {
                    foreach (var conf in this.Settings.POIMonster)
                    {
                        var text = conf.Key < 0 ? "Default" : $"Group {conf.Key}";
                        var shouldNotDelete = true;
                        if (ImGui.BeginTabItem(text, ref shouldNotDelete, ImGuiTabItemFlags.NoAssumedClosure))
                        {
                            conf.Value.Draw();
                            ImGui.EndTabItem();
                        }

                        if (conf.Key >= 0 && !shouldNotDelete)
                        {
                            this.poiMonsterConfigToDelete = conf.Key;
                            ImGui.OpenPopup("POIConfigHealthbarDeleteConfirmation");
                        }
                    }

                    this.DrawConfirmationPopup();
                    ImGui.EndTabBar();
                }
            }

            if (ImGui.CollapsingHeader("Player Configuration"))
            {
                if (ImGui.BeginTabBar("player_config"))
                {
                    foreach (var item in this.Settings.Player)
                    {
                        if (ImGui.BeginTabItem(item.Key))
                        {
                            item.Value.Draw();
                            ImGui.EndTabItem();
                        }
                    }

                    ImGui.EndTabBar();
                }
            }
        }

        /// <inheritdoc />
        public override void DrawUI()
        {
            var cAreaInstance = Core.States.InGameStateObject.CurrentAreaInstance;
            var cWorldInstance = Core.States.InGameStateObject.CurrentWorldInstance;
            if ((!this.Settings.DrawInTown && cWorldInstance.AreaDetails.IsTown) ||
                (!this.Settings.DrawInHideout && cWorldInstance.AreaDetails.IsHideout))
            {
                return;
            }

            if (Core.States.GameCurrentState != GameStateTypes.InGameState)
            {
                return;
            }

            if (!this.Settings.DrawWhenGameInBackground && !Core.Process.Foreground)
            {
                return;
            }

            if (Core.States.InGameStateObject.GameUi.SkillTreeNodesUiElements.Count > 0)
            {
                return;
            }

            this.UpdateOncePerDraw();
            foreach (var entity in cAreaInstance.AwakeEntities)
            {
                if (!entity.Value.IsValid || entity.Value.EntityState == EntityStates.Useless ||
                    entity.Value.EntityType == EntityTypes.Renderable ||
                    entity.Value.EntityState == EntityStates.PinnacleBossHidden)
                {
                    continue;
                }

                switch (entity.Value.EntityType)
                {
                    case EntityTypes.Player:
                        if (entity.Value.EntitySubtype == EntitySubtypes.PlayerOther)
                        {
                            if (entity.Value.EntityState == EntityStates.PlayerLeader)
                            {
                                this.DrawHealthbar(entity.Value, this.Settings.Player["leader"]);
                            }
                            else
                            {
                                this.DrawHealthbar(entity.Value, this.Settings.Player["member"]);
                            }
                        }
                        else
                        {
                            this.DrawHealthbar(entity.Value, this.Settings.Player["self"]);
                        }

                        break;
                    case EntityTypes.Monster:
                        if (entity.Value.EntitySubtype == EntitySubtypes.POIMonster)
                        {
                            if (!this.Settings.POIMonster.TryGetValue(entity.Value.EntityCustomGroup, out var poiConfig))
                            {
                                poiConfig = this.Settings.POIMonster[-1];
                            }

                            this.DrawHealthbar(entity.Value, poiConfig);
                        }
                        else if (entity.Value.EntityState == EntityStates.MonsterFriendly)
                        {
                            this.DrawHealthbar(entity.Value, this.Settings.Monster["friendly"]);
                        }
                        else if (entity.Value.EntityState != EntityStates.LegionStage0 &&
                            entity.Value.EntityState != EntityStates.LegionStage1Dead)
                        {
                            if (entity.Value.TryGetComponent<ObjectMagicProperties>(out var oComp))
                            {
                                switch (oComp.Rarity)
                                {
                                    case Rarity.Normal:
                                        this.DrawHealthbar(entity.Value, this.Settings.Monster["white"]);
                                        break;
                                    case Rarity.Magic:
                                        this.DrawHealthbar(entity.Value, this.Settings.Monster["magic"]);
                                        break;
                                    case Rarity.Rare:
                                        this.DrawHealthbar(entity.Value, this.Settings.Monster["rare"]);
                                        break;
                                    case Rarity.Unique:
                                        this.DrawHealthbar(entity.Value, this.Settings.Monster["unique"]);
                                        break;
                                }
                            }
                        }

                        break;
                }
            }
        }

        /// <inheritdoc />
        public override void OnDisable()
        {
            this.textures.cleanup(this.TexturesPath);
            this.onAreaChange?.Cancel();
            this.onAreaChange = null;
        }

        /// <inheritdoc />
        public override void OnEnable(bool isGameOpened)
        {
            this.textures.Load(this.TexturesPath);
            if (File.Exists(this.SettingPathname))
            {
                var content = File.ReadAllText(this.SettingPathname);
                this.Settings = JsonConvert.DeserializeObject<HealthBarsSettings>(content);
            }

            for (var i = 0; i < this.textureToValidate.Count; i++)
            {
                if (!this.textures.TextureKeys.Contains(this.textureToValidate[i]))
                {
                    throw new Exception($"Missing texture file {this.textureToValidate[i]} in {this.TexturesPath} folder.");
                }
            }

            // TODO: delete me at end of 2023 year
            this.Settings.Monster.Remove("POI");
            this.onAreaChange = CoroutineHandler.Start(this.OnAreaChange());
        }

        /// <inheritdoc />
        public override void SaveSettings()
        {
            Directory.CreateDirectory(Path.GetDirectoryName(this.SettingPathname));
            var settingsData = JsonConvert.SerializeObject(this.Settings, Formatting.Indented);
            File.WriteAllText(this.SettingPathname, settingsData);
        }

        private void DrawHealthbar(Entity entity, Config healthbarConfig)
        {
            if (!healthbarConfig.Enable)
            {
                return;
            }

            if (!entity.TryGetComponent<Render>(out var rComp))
            {
                return;
            }

            var curPos = rComp.WorldPosition;
            curPos.Z -= rComp.ModelBounds.Z + healthbarConfig.Shift.Y;
            var location = Core.States.InGameStateObject.CurrentWorldInstance.WorldToScreen(curPos);
            location.X = (float)Math.Round(location.X);
            location.Y = (float)Math.Round(location.Y);
            location.X += healthbarConfig.Shift.X;
            if (!entity.TryGetComponent<Life>(out var hComp))
            {
                return;
            }

            if (this.Settings.InterpolatePosition)
            {
                if (this.bPositions.TryGetValue(entity.Id, out var prevLocation))
                {
                    location = MathHelper.Lerp(prevLocation, location, 0.4f);
                }

                this.bPositions[entity.Id] = location;
            }

            var ptr = ImGui.GetBackgroundDrawList();
            var start = location - healthbarConfig.HalfOfScale;
            var end = location + healthbarConfig.HalfOfScale;
            ptr.AddRectFilled(start, end, ImGuiHelper.Color(healthbarConfig.BackgroundColor));

            var (hb_ptr, _, _) = this.textures.GetTexture(this.textureToValidate[0]);
            var hPercent = hComp.Health.CurrentInPercent();
            ptr.AddImage(hb_ptr, start, end - (Vector2.UnitX * healthbarConfig.Scale * (100 - hPercent) / 100f), Vector2.Zero, Vector2.One,
                (hPercent > this.Settings.CullingStrikeRange || !healthbarConfig.ShowCullStrike) ?
                ImGuiHelper.Color(healthbarConfig.HealthbarColor) :
                0xFFFFFFFF);

            if (hComp.EnergyShield.Total > 0)
            {
                var (es_ptr, _, _) = this.textures.GetTexture(this.textureToValidate[1]);
                ptr.AddImage(es_ptr, start, end - (Vector2.UnitX * healthbarConfig.Scale * (100 - hComp.EnergyShield.CurrentInPercent())  / 100f),
                    Vector2.Zero, Vector2.One,
                    ImGuiHelper.Color(healthbarConfig.ESColor));
            }

            var tmp = start - Vector2.UnitY;
            for (var i = 0; i < healthbarConfig.Graduations; i++)
            {
                tmp.X += healthbarConfig.GraduationsLocationStart;
                ptr.AddLine(tmp, tmp + healthbarConfig.GraduationsLocationEnd, 0xFF000000, this.graduationsThickness);
            }

            if (healthbarConfig.ShowText)
            {
                ptr.AddText(start - this.fontSize, ImGuiHelper.Color(healthbarConfig.TextColor),
                    this.healthToHumanReadable(hComp.Health.Current + hComp.EnergyShield.Current));
            }
        }

        private void UpdateOncePerDraw()
        {
            this.graduationsThickness = ImGui.GetFontSize() / 9f;
            this.fontSize = new(0f, ImGui.GetFontSize());
        }

        private string healthToHumanReadable(int value)
        {
            if (value >= 100000)
            {
                return $"{(value / 1000000f):0.00}M";

            }
            else if (value >= 100)
            {
                return $"{(value / 1000f):0.00}K";
            }
            else
            {
                return $"{value}";
            }
        }

        private void DrawConfirmationPopup()
        {
            ImGui.SetNextWindowPos(new Vector2(Core.Overlay.Size.Width / 3f, Core.Overlay.Size.Height / 3f));
            if (ImGui.BeginPopup("POIConfigHealthbarDeleteConfirmation"))
            {
                ImGui.Text($"Do you want to delete group {this.poiMonsterConfigToDelete} POI Monster healthbar config?");
                ImGui.Separator();
                if (ImGui.Button("Yes",
                    new Vector2(ImGui.GetContentRegionAvail().X / 2f, ImGui.GetTextLineHeight() * 2)))
                {
                    _ = this.Settings.POIMonster.Remove(poiMonsterConfigToDelete);
                    ImGui.CloseCurrentPopup();
                }

                ImGui.SameLine();
                if (ImGui.Button("No", new Vector2(ImGui.GetContentRegionAvail().X, ImGui.GetTextLineHeight() * 2)))
                {
                    ImGui.CloseCurrentPopup();
                }

                ImGui.EndPopup();
            }
        }

        private IEnumerator<Wait> OnAreaChange()
        {
            while (true)
            {
                yield return new Wait(RemoteEvents.AreaChanged);
                this.bPositions.Clear();
            }
        }
    }
}