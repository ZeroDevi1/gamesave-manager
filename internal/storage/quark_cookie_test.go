package storage

import "testing"

func TestParseQuarkCookie(t *testing.T) {
	cases := map[string]struct{ in, want string }{
		"header": {"__puus=a; __pus=b", "__puus=a; __pus=b"},
		"json":   {`[{"name":"__puus","value":"a"},{"name":"isQuark","value":"1"},{"name":"__pus","value":"b"}]`, "__puus=a; __pus=b"},
		"netscape": {
			"# Netscape HTTP Cookie File\n.quark.cn\tTRUE\t/\tFALSE\t1999999999\t__puus\ta\npan.quark.cn\tFALSE\t/\tTRUE\t1999999999\t__pus\tb\n",
			"__puus=a; __pus=b",
		},
		"netscape without newlines": {
			".quark.cn\tTRUE\t/\tFALSE\t1999999999\t__puus\tapan.quark.cn\tFALSE\t/\tTRUE\t1999999999\t__pus\tb",
			"__puus=a; __pus=b",
		},
	}
	for name, c := range cases {
		if got := parseQuarkCookie(c.in); got != c.want {
			t.Errorf("%s: got %q, want %q", name, got, c.want)
		}
	}
}

func TestSetCookieValue(t *testing.T) {
	if got := setCookieValue("a=1; __puus=old", "__puus", "new"); got != "a=1; __puus=new" {
		t.Errorf("replace: %q", got)
	}
	if got := setCookieValue("a=1;", "__pus", "x"); got != "a=1; __pus=x" {
		t.Errorf("append: %q", got)
	}
	if got := setCookieValue("", "__pus", "x"); got != "__pus=x" {
		t.Errorf("empty: %q", got)
	}
}
