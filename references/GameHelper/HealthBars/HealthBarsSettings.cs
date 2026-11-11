// <copyright file="HealthBarsSettings.cs" company="PlaceholderCompany">
// Copyright (c) PlaceholderCompany. All rights reserved.
// </copyright>

namespace HealthBars
{
    using System.Collections.Generic;
    using GameHelper.Plugin;

    /// <summary>
    ///     <see cref="HealthBars" /> plugin settings class.
    /// </summary>
    public sealed class HealthBarsSettings : IPSettings
    {
        /// <summary>
        ///     Draw Healthbars when in town.
        /// </summary>
        public bool DrawInTown = true;

        /// <summary>
        ///     Draw Healthbars when in hideout.
        /// </summary>
        public bool DrawInHideout = true;

        /// <summary>
        ///     Draw Healthbars when game is not foreground.
        /// </summary>
        public bool DrawWhenGameInBackground = false;

        /// <summary>
        ///     % health after which monster is cullable.
        /// </summary>
        public int CullingStrikeRange = 10;

        /// <summary>
        ///    Interpolate entity position to minimize flicker effect.
        /// </summary>
        public bool InterpolatePosition = false;

        /// <summary>
        ///     Healthbar config for monsters.
        /// </summary>
        public Dictionary<string, Config> Monster = new()
        {
            { "white",    new(new(0.5f, 0f, 0f, 1f)) },
            { "magic",    new(new(0f, 0.5f, 1f, 1f)) },
            { "rare",     new(new(1f, 1f, 0f, 1f), 9, true) },
            { "unique",   new(new(1f, 0.5f, 0f, 1f), 9, true) },
            { "friendly", new(new(0f, 1f, 0f, 1f)) },
        };

        /// <summary>
        ///     Healthbar config for POI monsters.
        /// </summary>
        public Dictionary<int, Config> POIMonster = new()
        {
            { -1, new(new(0.5f, 0.5f, 0.5f, 0.5f), 9, true) },
        };

        /// <summary>
        ///     Healthbar config for player.
        /// </summary>
        public Dictionary<string, Config> Player = new()
        {
            { "self", new(new(1f, 0f, 1f, 1f)) },
            { "leader", new(new(1f, 0f, 1f, 1f)) },
            { "member", new(new(0.5f, 1f, 0.5f, 1f)) },
        };
    }
}