// <copyright file="EntitySubtypes.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace GameHelper.RemoteEnums.Entity
{
    public enum EntitySubtypes
    {
        /// <summary>
        ///     All <see cref="EntityTypes"/> that are not sub-categorized yet.
        /// </summary>
        Unidentified,

        /// <summary>
        ///     All <see cref="EntityTypes"/> that doesn't require a sub-type.
        /// </summary>
        None,

        /// <summary>
        ///     A <see cref="EntityTypes.Player"/> that is controlled by the current user.
        /// </summary>
        PlayerSelf,

        /// <summary>
        ///     A <see cref="EntityTypes.Player"/> that is controlled by the other user.
        /// </summary>
        PlayerOther,

        /// <summary>
        ///     A <see cref="EntityTypes.Shrine"/> that is in The Viridian Wildwood and gives infinite buff to player.
        /// </summary>
        AzmeriSacrificeAltar,

        /// <summary>
        ///     A <see cref="EntityTypes.Shrine"/> that refill player flasks
        /// </summary>
        AzmeriWell,

        /// <summary>
        ///     A <see cref="EntityTypes.Shrine"/> that gives 5 mins buff to player experience gain.
        /// </summary>
        AzmeriExperienceGainShrine,

        /// <summary>
        ///     A <see cref="EntityTypes.Shrine"/> that gives 5 mins buff to player quantity and rarity.
        /// </summary>
        AzmeriIncreaseQuanityShrine,

        /// <summary>
        ///     A <see cref="EntityTypes.Shrine"/> that makes player immortal for 5 mins.
        /// </summary>
        AzmeriCanNotBeDamagedShrine,

        /// <summary>
        ///     Azmeri shrine that is not found yet.
        /// </summary>
        AzmeriUnknownShrine,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that converts the wisp to different types of wisp.
        /// </summary>
        AzmeriDustConvertor,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that reveal Azmeri terrain
        /// </summary>
        AzmeriLightBomb,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that refuels azmeri area.
        /// </summary>
        AzmeriRefuel,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that juice the map
        /// </summary>
        AzmeriBlueWispSml,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that juice the map
        /// </summary>
        AzmeriPurpleWispSml,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that juice the map
        /// </summary>
        AzmeriYellowWispSml,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that juice the map
        /// </summary>
        AzmeriBlueWispMed,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that juice the map
        /// </summary>
        AzmeriPurpleWispMed,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that juice the map
        /// </summary>
        AzmeriYellowWispMed,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that juice the map
        /// </summary>
        AzmeriBlueWispBig,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that juice the map
        /// </summary>
        AzmeriPurpleWispBig,

        /// <summary>
        ///     A <see cref="EntityTypes.ImportantMiscellaneousObject"/> that juice the map
        /// </summary>
        AzmeriYellowWispBig,

        /// <summary>
        ///     A <see cref="EntityTypes.Chest"/> that have labels on them.
        /// </summary>
        ChestWithLabel,

        /// <summary>
        ///     A <see cref="EntityTypes.Chest"/> you find in Delve.
        /// </summary>
        DelveChest,

        /// <summary>
        ///     A <see cref="EntityTypes.Chest"/> you find in expedition encounter.
        /// </summary>
        ExpeditionChest,

        /// <summary>
        ///     A <see cref="EntityTypes.Chest"/> you find in breach encounter.
        /// </summary>
        BreachChest,

        /// <summary>
        ///     A <see cref="EntityTypes.Chest"/> that is a Strongbox and low spawn rate.
        /// </summary>
        ImportantStrongbox,

        /// <summary>
        ///     A <see cref="EntityTypes.Chest"/> that is a Strongbox.
        /// </summary>
        Strongbox,

        /// <summary>
        ///     A <see cref="EntityTypes.Monster"/> that is a Epic chest and is in the Legion encounter.
        /// </summary>
        LegionEpicChest,

        /// <summary>
        ///     A <see cref="EntityTypes.Monster"/> that is a chest and is in the Legion encounter.
        /// </summary>
        LegionChest,

        /// <summary>
        ///     A <see cref="EntityTypes.NPC"/> that is very important to the user.
        /// </summary>
        SpecialNPC,

        /// <summary>
        ///     A <see cref="EntityTypes.NPC"/> that is in the viridian wildwood and have uniques to sell
        /// </summary>
        AzmeriTraderNPC,

        /// <summary>
        ///     A <see cref="EntityTypes.NPC"/> that is in the viridian wildwood and have harvest crops
        /// </summary>
        AzmeriHarvestNPC,

        /// <summary>
        ///     A <see cref="EntityTypes.Monster"/> that is in the Legion encounter.
        /// </summary>
        LegionMonster,

        /// <summary>
        ///    A <see cref="EntityTypes.Monster"/> that Einhar wants.
        /// </summary>
        YellowBestiaryMonster,

        /// <summary>
        ///    A <see cref="EntityTypes.Monster"/> that Einhar wants.
        /// </summary>
        RedBestiaryMonster,

        /// <summary>
        ///   A Harbinger league <see cref="EntityTypes.Monster"/> that can not be damaged.
        /// </summary>
        HarbingerMonster,

        /// <summary>
        ///     A monster that is point of interest for the user.
        /// </summary>
        POIMonster,

        /// <summary>
        ///     A <see cref="EntityTypes.Monster"/> that is Tormented spirit.
        /// </summary>
        TormentedSpiritsMonster,

        /// <summary>
        ///    Betrayal league unique enemy NPCs.
        /// </summary>
        BetrayalEnemyNPC,

        /// <summary>
        ///    Atlas pinnacle boss.
        /// </summary>
        PinnacleBoss,

        /// <summary>
        ///     A <see cref="EntityTypes.Item"/> that is on the ground.
        /// </summary>
        WorldItem,

        /// <summary>
        ///      A <see cref="EntityTypes.Item"/> that is in the inventory.
        /// </summary>
        InventoryItem,
        AbyssCrack,
        AbyssMidNode,
        AbyssFinalNode,
        AbyssStartNode,
    }
}
