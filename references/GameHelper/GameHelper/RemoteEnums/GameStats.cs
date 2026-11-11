// <copyright file="GameStats.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace GameHelper.RemoteEnums
{
    /// <summary>
    ///     Stats enum taken from GGPK file -> Stats.dat -> Id Column
    ///     and made c# friendly via the sed bash command.
    /// </summary>
    /// NOTE: sed commad -> sed 's/+/positive_/g; s/%/percentage/g; s/-/negative_/g; s/$/,/g;'
    /// NOTE: Do not forget to add '= 1' on the first enum.
    public enum GameStats
    {
#pragma warning disable CS1591, SA1602
        // flask_charges_used_+% whatever you read in _rid, add +1 to it.
        // offset finder tool now have this value -> Charges group
        flask_charges_used_positive_percentage = 0x2E5,
        is_capturable_monster = 0x22B9, // value in stats.dat._rid + 1
        is_bestiary_yellow_beast = 0x4290, // value in stats.dat._rid + 1
#pragma warning restore CS1591, SA1602
    }
}
