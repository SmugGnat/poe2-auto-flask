// <copyright file="Targetable.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace GameHelper.RemoteObjects.Components
{
    using System;
    using GameOffsets.Objects.Components;
    using ImGuiNET;

    /// <summary>
    ///     The <see cref="Targetable" /> component in the entity.
    /// </summary>
    public class Targetable : ComponentBase
    {
        private uint targetableFlag = 0x00;
        private uint hiddenFlag = 0x00;

        /// <summary>
        ///     Initializes a new instance of the <see cref="Targetable" /> class.
        /// </summary>
        /// <param name="address">address of the <see cref="Targetable" /> component.</param>
        public Targetable(IntPtr address)
            : base(address) { }

        /// <summary>
        ///     Gets the value indicating whether the entity is targetable or not.
        /// </summary>
        public bool IsTargetable { get; private set; } = false;

        /// <summary>
        ///     Converts the <see cref="Targetable" /> class data to ImGui.
        /// </summary>
        internal override void ToImGui()
        {
            base.ToImGui();
            ImGui.Text($"Targetable Flag: {Convert.ToString(this.targetableFlag, 2),8}");
            ImGui.Text($"Hidden Flag: {Convert.ToString(this.hiddenFlag, 2),8}");
            ImGui.Text($"Is Targetable {this.IsTargetable}");
        }

        /// <inheritdoc />
        protected override void UpdateData(bool hasAddressChanged)
        {
            var reader = Core.Process.Handle;
            var data = reader.ReadMemory<TargetableOffsets>(this.Address);
            this.OwnerEntityAddress = data.Header.EntityPtr;
            this.targetableFlag = data.TargetableFlag;
            this.hiddenFlag = data.HiddenFlag;
            this.IsTargetable = TargetableHelper.IsTargetable(data.TargetableFlag);
        }
    }
}