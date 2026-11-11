// <copyright file="WorldDrawingCore.cs" company="PlaceholderCompany">
// Copyright (c) PlaceholderCompany. All rights reserved.
// </copyright>

namespace WorldDrawing
{
    using System;
    using System.Collections.Generic;
    using System.Diagnostics;
    using System.IO;
    using System.Numerics;
    using Coroutine;
    using GameHelper;
    using GameHelper.CoroutineEvents;
    using GameHelper.Plugin;
    using GameHelper.RemoteEnums;
    using GameHelper.RemoteEnums.Entity;
    using GameHelper.RemoteObjects.Components;
    using GameHelper.Utils;
    using ImGuiNET;
    using Newtonsoft.Json;


    /// <summary>
    /// <see cref="WorldDrawingCore"/> plugin.
    /// </summary>
    public sealed class WorldDrawingCore : PCore<WorldDrawingSettings>
    {
        private string SettingPathname => Path.Join(this.DllDirectory, "config", "settings.txt");
        private readonly HashSet<uint> knownAbyssStarts = new();
        private readonly List<List<(uint id, EntitySubtypes nodeType, Vector2 worldPos, float height)>> abyssNodes = new(20);
        private readonly Dictionary<uint, Stopwatch> abyssNodesStopwatches = new();
        private readonly Dictionary<uint, string> abyssNodeResult = new();
        private ActiveCoroutine onAreaChangeCoroutine;

        /// <inheritdoc/>
        public override void DrawSettings()
        {

            this.DrawAbyssPathConfig();
        }

        /// <inheritdoc/>
        public override void DrawUI()
        {
            if (Core.States.InGameStateObject.GameUi.SkillTreeNodesUiElements.Count > 0)
            {
                return;
            }

            this.DrawAbyssPaths();
        }

        /// <inheritdoc/>
        public override void OnDisable()
        {
            this.onAreaChangeCoroutine?.Cancel();
            this.onAreaChangeCoroutine = null;
        }

        /// <inheritdoc/>
        public override void OnEnable(bool isGameOpened)
        {
            if (File.Exists(this.SettingPathname))
            {
                var content = File.ReadAllText(this.SettingPathname);
                this.Settings = JsonConvert.DeserializeObject<WorldDrawingSettings>(content);
            }

            this.onAreaChangeCoroutine = CoroutineHandler.Start(this.onAreaChange());
        }

        /// <inheritdoc/>
        public override void SaveSettings()
        {
            Directory.CreateDirectory(Path.GetDirectoryName(this.SettingPathname));
            var settingsData = JsonConvert.SerializeObject(this.Settings, Formatting.Indented);
            File.WriteAllText(this.SettingPathname, settingsData);
        }

        private void DrawAbyssPathConfig()
        {
            ImGui.NewLine();
            if (ImGui.CollapsingHeader("Abyss Lines"))
            {
                ImGui.Checkbox($"Show while large map is visible", ref this.Settings.OnlyShowAbyssPathWhenLargeMapHidden);
                for (var i = 0; i < this.Settings.AbyssPath.Length; i++)
                {
                    if (ImGui.CollapsingHeader($"Abyss Path {i}"))
                    {
                        ImGui.Checkbox($"Enable##AbyssPath{i}", ref this.Settings.AbyssPath[i].enable);
                        ImGui.DragFloat($"Line Width##AbyssPath{i}", ref this.Settings.AbyssPath[i].width);
                        ImGui.ColorEdit4($"Line Color##AbyssPath{i}", ref this.Settings.AbyssPath[i].color);
                    }
                }
            }
        }

        private void DrawAbyssPaths()
        {
            if (Core.States.GameCurrentState != GameStateTypes.InGameState ||
                Core.States.InGameStateObject.CurrentWorldInstance.AreaDetails.IsTown ||
                Core.States.InGameStateObject.CurrentWorldInstance.AreaDetails.IsHideout ||
                Core.States.InGameStateObject.GameUi.SkillTreeNodesUiElements.Count > 0)
            {
                return;
            }

            foreach (var entity in Core.States.InGameStateObject.CurrentAreaInstance.AwakeEntities)
            {
                if (entity.Value.EntitySubtype != EntitySubtypes.AbyssCrack &&
                    entity.Value.EntitySubtype != EntitySubtypes.AbyssFinalNode &&
                    entity.Value.EntitySubtype != EntitySubtypes.AbyssMidNode &&
                    entity.Value.EntitySubtype != EntitySubtypes.AbyssStartNode)
                {
                    continue;
                }

                if (!entity.Value.TryGetComponent<Render>(out var render))
                {
                    continue;
                }

                if (entity.Value.EntitySubtype == EntitySubtypes.AbyssStartNode)
                {
                    if (!this.knownAbyssStarts.Contains(entity.Value.Id))
                    {
                        this.abyssNodes.Add(new()
                        {
                            (entity.Value.Id, entity.Value.EntitySubtype, new(render.WorldPosition.X, render.WorldPosition.Y), render.TerrainHeight)
                        });

                        this.knownAbyssStarts.Add(entity.Value.Id);
                    }

                    continue;
                }

                for (var i = 0; i < this.abyssNodes.Count; i++)
                {
                    if (entity.Value.Id - 1 == this.abyssNodes[i][^1].id)
                    {
                        this.abyssNodes[i].Add((
                            entity.Value.Id,
                            entity.Value.EntitySubtype,
                            new(render.WorldPosition.X, render.WorldPosition.Y),
                            render.TerrainHeight));
                        break;
                    }
                }

                if (!entity.Value.IsValid)
                {
                    continue;
                }

                if ((entity.Value.EntitySubtype == EntitySubtypes.AbyssMidNode ||
                    entity.Value.EntitySubtype == EntitySubtypes.AbyssFinalNode) &&
                    entity.Value.TryGetComponent<Transitionable>(out var trans))
                {
                    if (trans.CurrentState == 2 && !this.abyssNodesStopwatches.ContainsKey(entity.Value.Id))
                    {
                        this.abyssNodesStopwatches[entity.Value.Id] = Stopwatch.StartNew();
                    }
                    else if ((trans.CurrentState == 3 || trans.CurrentState == 4) && this.abyssNodesStopwatches.TryGetValue(entity.Value.Id, out var value))
                    {
                        value.Stop();
                        this.abyssNodeResult.TryAdd(entity.Value.Id, trans.CurrentState == 3 ? "fail" : "pass");
                    }
                }
            }

            for (var i = 0; i < this.abyssNodes.Count; i++)
            {
                var isFinalNodeFound = false;
                for (var j = 1; j < this.abyssNodes[i].Count; j++)
                {
                    var (cId, cEST, cPos, cHeight) = this.abyssNodes[i][j];
                    var (_, pEST, pPos, pHeight) = this.abyssNodes[i][j - 1];
                    var cLoc = Core.States.InGameStateObject.CurrentWorldInstance.WorldToScreen(cPos, cHeight);
                    var pLoc = Core.States.InGameStateObject.CurrentWorldInstance.WorldToScreen(pPos, pHeight);
                    if (isFinalNodeFound)
                    {
                        // Should get true after final node is found.
                        break;
                    }
                    else
                    {
                        isFinalNodeFound = cEST == EntitySubtypes.AbyssFinalNode;
                    }

                    if (cLoc.X < Core.Process.WindowArea.X || cLoc.X > Core.Process.WindowArea.Width ||
                        cLoc.Y < Core.Process.WindowArea.Y || cLoc.Y > Core.Process.WindowArea.Height)
                    {
                        continue;
                    }

                    if (pLoc.X < Core.Process.WindowArea.X || pLoc.X > Core.Process.WindowArea.Width ||
                        pLoc.Y < Core.Process.WindowArea.Y || pLoc.Y > Core.Process.WindowArea.Height)
                    {
                        continue;
                    }

                    if (this.Settings.OnlyShowAbyssPathWhenLargeMapHidden && Core.States.InGameStateObject.GameUi.LargeMap.IsVisible)
                    {
                    }
                    else if (this.Settings.AbyssPath[i].enable)
                    {
                        ImGui.GetBackgroundDrawList().AddLine(pLoc, cLoc, ImGuiHelper.Color(this.Settings.AbyssPath[i].color), this.Settings.AbyssPath[i].width);
                    }

                    if ((cEST == EntitySubtypes.AbyssMidNode ||
                        cEST == EntitySubtypes.AbyssFinalNode) &&
                        this.abyssNodesStopwatches.TryGetValue(cId, out var sw))
                    {
                        var txtToDisplay = $"  {(int)(sw.ElapsedMilliseconds / 1000f)}  ";
                        if (!sw.IsRunning)
                        {
                            txtToDisplay += $"- ({this.abyssNodeResult[cId]})";
                        }

                        ImGui.GetBackgroundDrawList().AddRectFilled(cLoc, cLoc + ImGui.CalcTextSize(txtToDisplay),
                            ImGuiHelper.Color(255, 255, 255, 255));
                        ImGui.GetBackgroundDrawList().AddText(cLoc, ImGuiHelper.Color(0, 0, 0, 255), txtToDisplay);
                    }
                }
            }
        }

        private IEnumerable<Wait> onAreaChange()
        {
            while (true)
            {
                yield return new Wait(RemoteEvents.AreaChanged);
                this.ClearAll();
            }
        }

        private void ClearAll()
        {
            for (var i = 0; i < this.abyssNodes.Count; i++)
            {
                this.abyssNodes[i].Clear();
            }

            this.abyssNodes.Clear();
            this.knownAbyssStarts.Clear();
            this.abyssNodesStopwatches.Clear();
            this.abyssNodeResult.Clear();
        }
    }
}
