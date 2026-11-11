// <copyright file="ImportantUiElements.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace GameHelper.RemoteObjects.States.InGameStateObjects
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Coroutine;
    using CoroutineEvents;
    using GameHelper.Cache;
    using GameHelper.Utils;
    using GameOffsets.Objects.States.InGameState;
    using ImGuiNET;
    using RemoteEnums;
    using UiElement;

    /// <summary>
    ///     This is actually UiRoot main child which contains
    ///     all the UiElements (100+). Normally it's at index 1 of UiRoot.
    ///     This class is created because traversing childrens of
    ///     UiRoot is a slow process that requires lots of memory reads.
    ///     Drawback:
    ///     1: Every league/patch the offsets needs to be updated.
    ///     Offsets used over here are very unstable.
    ///     2: More UiElements we are tracking = More memory read
    ///     every X seconds just to update IsVisible :(.
    /// </summary>
    public class ImportantUiElements : RemoteObjectBase
    {
        private readonly UiElementParents rootCache;
        private readonly UiElementParents passiveSkillTreeCache;

        /// <summary>
        ///     Passive skill tree node Parent UI element.
        ///     UiRoot -> MainChild -> index 28 -> 1
        /// </summary>
        private UiElementBase passiveskilltreenodes;

        /// <summary>
        ///     Initializes a new instance of the <see cref="ImportantUiElements" /> class.
        /// </summary>
        /// <param name="address">
        ///     UiRoot 1st child address (starting from 0)
        ///     or <see cref="IntPtr.Zero" /> in case UiRoot has no child.
        /// </param>
        internal ImportantUiElements(IntPtr address)
            : base(address)
        {
            this.rootCache = new(null, GameStateTypes.InGameState, GameStateTypes.EscapeState);
            this.passiveSkillTreeCache = new(this.rootCache, GameStateTypes.InGameState, GameStateTypes.EscapeState);

            this.passiveskilltreenodes = new(IntPtr.Zero, this.rootCache);
            this.LargeMap = new(IntPtr.Zero, this.rootCache);
            this.MiniMap = new(IntPtr.Zero, this.rootCache);
            this.ChatParent = new(IntPtr.Zero, this.rootCache);

            this.SkillTreeNodesUiElements = new();
            Core.CoroutinesRegistrar.Add(CoroutineHandler.Start(
                this.OnPerFrame(), "[InGameState] Update ImportantUiElements", priority: int.MaxValue - 3));
        }

        /// <summary>
        ///     Gets the LargeMap UiElement.
        ///     UiRoot -> MainChild -> 3rd index -> 1nd index.
        /// </summary>
        public LargeMapUiElement LargeMap { get; }

        /// <summary>
        ///     Gets the MiniMap UiElement.
        ///     UiRoot -> MainChild -> 3rd index -> 2nd index.
        /// </summary>
        public MapUiElement MiniMap { get; }

        /// <summary>
        ///     Gets the Chat UiElement parent.
        ///     UiRoot -> MainChild -> this index keeps moving around
        /// </summary>
        public ChatParentUiElement ChatParent { get; }

        /// <summary>
        ///     Gets the skill tree nodes UI Elements.
        ///     UiRoot -> MainChild -> index 28 -> 2 -> all childrens;
        /// </summary>
        public List<SkillTreeNodeUiElement> SkillTreeNodesUiElements { get; }

        internal override void ToImGui()
        {
            this.displayParentsCache();
            base.ToImGui();
            ImGui.Text($"Passive Skill Tree Panel Visible: {this.passiveskilltreenodes.IsVisible}");
            ImGui.Text($"Total Skill Tree Nodes: {this.SkillTreeNodesUiElements.Count}");
            if (ImGui.TreeNode("Skill Tree Nodes"))
            {
                for (var i = 0; i < this.SkillTreeNodesUiElements.Count; i++)
                {
                    var skillId = this.SkillTreeNodesUiElements[i].SkillGraphId;
                    ImGuiHelper.DisplayTextAndCopyOnClick($"index: {i}, skillId: {skillId}", $"{skillId}");
                    ImGui.GetForegroundDrawList().AddText(this.SkillTreeNodesUiElements[i].Postion, 0xFF0000FF, $"{i}");
                }

                ImGui.TreePop();
            }
        }

        /// <inheritdoc />
        protected override void CleanUpData()
        {
            this.passiveskilltreenodes.Address = IntPtr.Zero;
            this.MiniMap.Address = IntPtr.Zero;
            this.LargeMap.Address = IntPtr.Zero;
            this.ChatParent.Address = IntPtr.Zero;
            this.SkillTreeNodesUiElements.Clear();
        }

        /// <inheritdoc />
        protected override void UpdateData(bool hasAddressChanged)
        {
            this.UpdateParentsCache();
            var reader = Core.Process.Handle;
            var data1 = reader.ReadMemory<ImportantUiElementsOffsets>(Core.GHSettings.IsTaiwanClient ? this.Address - 0x08 : this.Address);
            if (Core.GHSettings.EnableControllerMode)
            {
                this.LargeMap.Address = reader.ReadMemory<MapParentStruct>(data1.ControllerModeMapParentPtr).LargeMapPtr;
                this.MiniMap.Address = IntPtr.Zero;
                this.ChatParent.Address = IntPtr.Zero;
                this.passiveskilltreenodes.Address = IntPtr.Zero;
            }
            else
            {
                var data2 = reader.ReadMemory<MapParentStruct>(data1.MapParentPtr);
                var data3 = reader.ReadMemory<PassiveSkillTreeStruct>(data1.PassiveSkillTreePanel);
                // This won't throw an exception (i.e. this address is not a UIElement) because (lucky us)
                // game UiElement garbage collection is not instant. if this ever changes, put try catch on it.
                this.LargeMap.Address = data2.LargeMapPtr;
                this.MiniMap.Address = data2.MiniMapPtr;
                this.ChatParent.Address = data1.ChatParentPtr;
                this.passiveskilltreenodes.Address = data3.SkillTreeNodeUiElements;
                this.updatePassiveSkillTreeData();
            }
        }

        private void updatePassiveSkillTreeData()
        {
            if (this.passiveskilltreenodes.IsVisible)
            {
                this.AddOrUpdateSkillNodes();
            }
            else
            {
                this.ClearSkillNodes();
            }
        }

        private void ClearSkillNodes()
        {
            this.SkillTreeNodesUiElements.Clear();
            this.passiveSkillTreeCache.Clear();
        }

        private void AddOrUpdateSkillNodes()
        {
            if (this.SkillTreeNodesUiElements.Count == 0)
            {
                var currentChild = new UiElementBase(IntPtr.Zero, this.passiveSkillTreeCache);
                for (var i = 3; i < this.passiveskilltreenodes.TotalChildrens; i++)
                {
                    currentChild.Address = this.passiveskilltreenodes[i].Address;
                    if (!currentChild.IsVisible)
                    {
                        break;
                    }

                    this.AddSkillTreeNodeUiElementRecursive(currentChild);
                }
            }
            else
            {
                Parallel.For(0, this.SkillTreeNodesUiElements.Count, (i) =>
                {
                    this.SkillTreeNodesUiElements[i].Address = this.SkillTreeNodesUiElements[i].Address;
                });
            }
        }

        private void AddSkillTreeNodeUiElementRecursive(UiElementBase uie)
        {
            if (uie.TotalChildrens > 0)
            {
                for (var i = 0; i < uie.TotalChildrens; i++)
                {
                    this.AddSkillTreeNodeUiElementRecursive(uie[i]);
                }
            }
            else
            {
                var skillNode = new SkillTreeNodeUiElement(uie.Address, this.passiveSkillTreeCache);
                if (skillNode.SkillGraphId != 0)
                {
                    this.SkillTreeNodesUiElements.Add(skillNode);
                }
            }
        }

        private void displayParentsCache()
        {
            this.rootCache.ToImGui();
            this.passiveSkillTreeCache.ToImGui();
        }

        private void UpdateParentsCache()
        {
            this.rootCache.UpdateAllParentsParallel();
            this.passiveSkillTreeCache.UpdateAllParentsParallel();
        }

        private IEnumerator<Wait> OnPerFrame()
        {
            while (true)
            {
                yield return new Wait(GameHelperEvents.PerFrameDataUpdate);
                if (this.Address != IntPtr.Zero &&
                    Core.States.GameCurrentState == GameStateTypes.InGameState)
                {
                    // sending false because "true" use-case is handled
                    // by UpdateData function when address actually gets changed.
                    this.UpdateData(false);
                }
            }
        }
    }
}