package game

import (
	"reflect"
	"testing"
)

func TestParseWindowsSavePaths(t *testing.T) {
	wikitext := `===Save game data location===
{{Game data|
{{Game data/saves|Windows|{{p|userprofile}}\Saved Games\kingdomcome2\saves\|Steam version}}
{{Game data/saves|Windows|{{p|appdata}}\Foo\{{p|uid}}\}}
{{Game data/saves|Linux|~/.local/share/foo}}
}}
{{--}}Cloud sync supported.
===Next section===
{{Game data/saves|Windows|C:\ignored}}
|Steam AppID = 1771300
`
	paths, notes := parseWindowsSavePaths(wikitext)
	want := []string{"%USERPROFILE%/Saved Games/kingdomcome2/saves/", "%APPDATA%/Foo/*/"}
	if !reflect.DeepEqual(paths, want) {
		t.Fatalf("paths = %q, want %q", paths, want)
	}
	if notes == nil || *notes != "Cloud sync supported" {
		t.Fatalf("notes = %v", notes)
	}
	if id := steamAppIDFromWikitext(wikitext); id == nil || *id != 1771300 {
		t.Fatalf("appid = %v", id)
	}
}
