namespace VniDrop.Core;

public static class FileCountPlural
{
    public static string Category(ulong count, string language) => language.Split('-')[0] switch
    {
        "ru" when count % 10 == 1 && count % 100 != 11 => "one",
        "ru" or "pl" when count % 10 is >= 2 and <= 4 && count % 100 is not (>= 12 and <= 14) => "few",
        "pl" when count == 1 => "one",
        "ru" or "pl" => "many",
        "fr" when count <= 1 => "one",
        _ => count == 1 ? "one" : "other",
    };
}
