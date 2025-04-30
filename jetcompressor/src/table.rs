// Strings I'd like to add but can't:
// * com. - conflicts with commit
// * blob - conflicts with block
#[rustfmt::skip]
pub const PRESET_SYMBOLS: [&str; 255] = [
// Skip all the ASCII control characters (except newline).  Use the space for
// numerical digraphs (for dates and times).
"00"     , "01"       , "02"       , "03"       , "04"       , "05"      , "06"       , "07"      ,
"08"     , "09"       , "\n"       , "10"       , "11"       , "12"      , "13"       , "14"      ,
"15"     , "16"       , "17"       , "18"       , "19"       , "20"      , "21"       , "22"      ,
"23"     , "24"       , "25"       , "26"       , "27"       , "28"      , "29"       , "30"      ,
// Include all the printable ASCII characters (except DEL; isn't that more of a
// control character?)
" "      , "!"        , "\""       , "#"        , "$"        , "%"       , "&"        , "'"       ,
"("      , ")"        , "*"        , "+"        , ","        , "-"       , "."        , "/"       ,
"0"      , "1"        , "2"        , "3"        , "4"        , "5"       , "6"        , "7"       ,
"8"      , "9"        , ":"        , ";"        , "<"        , "="       , ">"        , "?"       ,
"@"      , "A"        , "B"        , "C"        , "D"        , "E"       , "F"        , "G"       ,
"H"      , "I"        , "J"        , "K"        , "L"        , "M"       , "N"        , "O"       ,
"P"      , "Q"        , "R"        , "S"        , "T"        , "U"       , "V"        , "W"       ,
"X"      , "Y"        , "Z"        , "["        , "\\"       , "]"       , "^"        , "_"       ,
"`"      , "a"        , "b"        , "c"        , "d"        , "e"       , "f"        , "g"       ,
"h"      , "i"        , "j"        , "k"        , "l"        , "m"       , "n"        , "o"       ,
"p"      , "q"        , "r"        , "s"        , "t"        , "u"       , "v"        , "w"       ,
"x"      , "y"        , "z"        , "{"        , "|"        , "}"       , "~"        , "31"      ,
// Keep going with the digraphs, up to 59
"32"     , "33"       , "34"       , "35"       , "36"       , "37"      , "38"       , "39"      ,
"40"     , "41"       , "42"       , "43"       , "44"       , "45"      , "46"       , "47"      ,
"48"     , "49"       , "50"       , "51"       , "52"       , "53"      , "54"       , "55"      ,
// Various JSON fragments
"56"     , "57"       , "58"       , "59"       , ",\""      , ":["      , ":{"       , "],"      ,
"{\""    , "},"       , "\":"      , "\","      , "\",\""    , "\":\""   , "\":{\""   , "\"}}"    , 
// Language codes based on number of speakers
"langs"  , "en"       , "zh"       , "hi"       , "es"       , "ar"      , "fr"       , "pt"      ,
"ru"     , "id"       , "ur"       , "de"       , "ja"       , "vi"      , "ko"       , "it"      ,
// Common bits of URIs
"uri"    , "http"     , "://"      , "www."     , "app.bsky" , "bsky"    , "atproto"  , ".graph." ,
"feed"   , ".feed."   , ".repo."   , "social"   , "at://"    , "did"     , ":plc:"    , "media"   ,
// More keys
"opera"  , "tion"     , "commit"   , "delete"   , "update"   , "time_us" , "account"  , "subject" ,
"kind"   , "like"     , "post"     , "reply"    , "follow"   , "block"   , "create"   , "dAt"     ,
"collec" , "rkey"     , "\"record" , "root"     , "seq"      , "$type"   , "#link"    , "text"    ,
"cid"    , "bafyrei"  , "type"     , "actor"    , "profile"  , "handle"  , "parent"   , "strong"  ,
// Stuff for embedded media
"embed"  , "mimeType" , "image"    , "jpeg"     , "png"      , "richtext", "index"    , "span"    ,
"byte"   , "Slice"    , "Start"    , "End"      , "facet"    , "size"    , "features" , "$link"   ,
"aspect" , "Ratio"    , "thumb"    , "height"   , "tag"      , "alt"     , "external" ,  /* reserved */
];
