// <copyright file="Stats.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace GameHelper.RemoteObjects.Components
{
    using System;
    using System.Collections.Generic;
    using GameHelper.RemoteEnums;
    using GameHelper.Utils;
    using GameOffsets.Objects.Components;

    /// <summary>
    ///     The <see cref="Stats" /> component in the entity.
    /// </summary>
    public class Stats : ComponentBase
    {
        /// <summary>
        ///     Gets all the stats of the entity.
        /// </summary>
        public Dictionary<GameStats, int> AllStats = new();

        /// <summary>
        ///     Initializes a new instance of the <see cref="Stats" /> class.
        /// </summary>
        /// <param name="address">address of the <see cref="Stats" /> component.</param>
        public Stats(IntPtr address)
            : base(address) { }

        /// <inheritdoc/>
        internal override void ToImGui()
        {
            base.ToImGui();
            ImGuiHelper.StatsWidget(this.AllStats, "Entity Stats");
        }

        /// <inheritdoc/>
        protected override void UpdateData(bool hasAddressChanged)
        {
            var reader = Core.Process.Handle;
            var data = reader.ReadMemory<StatsOffsets>(this.Address);
            this.OwnerEntityAddress = data.Header.EntityPtr;
            if (data.StatsDataPtr != IntPtr.Zero)
            {
                var data2 = reader.ReadMemory<StatsStructInternal>(data.StatsDataPtr);
                base.StatUpdator(this.AllStats, data2.Stats);
            }
        }
    }
}
