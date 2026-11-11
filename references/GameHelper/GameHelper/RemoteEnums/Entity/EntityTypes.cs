// <copyright file="EntityTypes.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace GameHelper.RemoteEnums.Entity
{
    /// <summary>
    ///     Enum for entity Categorization system.
    /// </summary>
    public enum EntityTypes
    {
        /// <summary>
        ///     Unknown entity type i.e. entity isn't categorized yet.
        /// </summary>
        Unidentified,

        /// <summary>
        ///     Contains the Chest component.
        /// </summary>
        Chest,

        /// <summary>
        ///     Entity containing NPCs component and don't have OMP component.
        /// </summary>
        NPC,

        /// <summary>
        ///     Contains the Player component
        /// </summary>
        Player,

        /// <summary>
        ///     Contains the Shrine component.
        /// </summary>
        Shrine,

        /// <summary>
        ///     There are a lot of miscellaneous objects, the purpose of this entitytype is to filter out the important ones.
        /// </summary>
        ImportantMiscellaneousObject,

        /// <summary>
        ///     Contains the TriggerableBlockage component.
        /// </summary>
        Blockage,

        /// <summary>
        ///     Contains life and ObjectMagicProperties component.
        /// </summary>
        Monster,

        /// <summary>
        ///     Entity that creates bomb in delirium when player goes near it.
        /// </summary>
        DeliriumBomb,

        /// <summary>
        ///     Entity that spawn monster in delirium when player goes near it.
        /// </summary>
        DeliriumSpawner,

        /// <summary>
        ///     All of the items (i.e. on the ground or in the inventory).
        /// </summary>
        Item,

        /// <summary>
        ///     All the entities on the ground that are renderable.
        /// </summary>
        Renderable,
    }
}
