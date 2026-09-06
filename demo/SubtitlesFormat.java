import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileWriter;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Map.Entry;
import java.util.regex.Pattern;
import java.util.TreeMap;

public class SubtitlesFormat {
	public static final String INPUT_ROOT = "\\剧集\\美剧\\行尸走肉\\第08季\\";
	public static final String OUTPUT_ROOT = "H:";
	public static final String SEPARATOR = "\\N{\\fnArial\\fs30}";
	public static final String SEPARATOR_REGEX = "\\\\N\\{\\\\fnArial\\\\fs30\\}";
	public static final String ENTER = "\r\n";

	public static void main(String[] args) throws Exception {
		search(new File(System.getProperty("user.dir") + "\\src\\subtitles" + INPUT_ROOT), "");
	}

	private static void search(File dir, String spance) throws Exception {
		for (File file : dir.listFiles()) {
			if (file.isFile()) {
				new File(OUTPUT_ROOT + INPUT_ROOT + file.getAbsolutePath().split(INPUT_ROOT.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)"))[1]).delete();
				try {
					format(file);
				} catch (Exception e) {
					e.printStackTrace();
					System.err.println(spance + file.getName() + " is Failed");
					continue;
				}
				System.out.println(spance + file.getName() + " is Done");
			} else if (file.isDirectory()) {
				System.out.println(spance + file.getName());
				search(file, "|--" + spance);
			}
		}
	}

	private static void format(File file) throws Exception {
		try (InputStream inputStream = new FileInputStream(file.getAbsolutePath()); InputStreamReader inputStreamReader = new InputStreamReader(inputStream, "UTF-8"); BufferedReader reader = new BufferedReader(inputStreamReader)) {
			List<String> lines = merge(reader);
			String content = build(lines);
			if (!content.contains(file.getName().substring(7).replace(".ass", "》"))) {
				System.err.println("[标题缺失] " + file.getName());
			}
			write(OUTPUT_ROOT + INPUT_ROOT + file.getAbsolutePath().split(INPUT_ROOT.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)"))[1], content);
		}
	}

	private static List<String> merge(BufferedReader reader) throws Exception {
		String line = null;
		Map<String, List<String>> treeMap = new TreeMap<>();
		while ((line = reader.readLine()) != null) {
			if (line.startsWith("Dialogue") && !line.endsWith(",,")) {
				line = filter(line);
				String key = line.substring(12, 33);
				if (treeMap.containsKey(key)) {
					List<String> lines = treeMap.get(key);
					if (hasChinese(line)) {
						String value = lines.get(0);
						lines.set(0, line.substring(33));
						lines.add(value);
					} else {
						lines.add(line.substring(33));
					}
				} else {
					List<String> lines = new ArrayList<>();
					lines.add(line.substring(33));
					treeMap.put(key, lines);
				}
			}
		}
		List<String> lines = new ArrayList<>();
		for (Entry<String, List<String>> entry : treeMap.entrySet()) {
			String[] values = entry.getValue().get(0).split(",,");
			StringBuilder builder = new StringBuilder("Dialogue: 0,");
			builder.append(entry.getKey());
			builder.append(",Default,0,0,0,0,,");
			builder.append(values[values.length - 1].replaceAll("(\\\\N)([\\w\\W]*)}", SEPARATOR_REGEX));
			if (entry.getValue().size() == 2) {
				values = entry.getValue().get(1).split(",,");
				builder.append(SEPARATOR).append(values[values.length - 1]);
			}
			lines.add(builder.toString());
		}
		for (int index = 0; index < lines.size(); index++) {
			if (index > 0 && lines.get(index).split(SEPARATOR_REGEX).length == 2 && !lines.get(index - 1).endsWith("...")) {
				wrap(lines.get(index).split(SEPARATOR_REGEX)[1].trim(), lines, index);
				if (lines.get(index).contains("- ")) {
					for (String sentence : lines.get(index).split(SEPARATOR_REGEX)[1].split("- ")) {
						wrap(sentence.trim(), lines, index);
					}
				}
			}
		}
		return check(lines);
	}

	private static void wrap(String sentence, List<String> lines, int index) {
		if (sentence.length() > 0 && isLowerCase(sentence.charAt(0))) {
			if (lines.get(index - 1).endsWith(",") || lines.get(index - 1).endsWith(")")) {
			} else if (lines.get(index - 1).endsWith("!") || lines.get(index - 1).endsWith("?")) {
				char[] chars = sentence.toCharArray();
				chars[0] -= 32;
				lines.set(index, lines.get(index).replace(sentence, String.valueOf(chars)));
			} else {
				lines.set(index - 1, lines.get(index - 1) + "...");
			}
		}
	}

	private static List<String> check(List<String> lines) {
		for (int index = 0; index < lines.size(); index++) {
			if (index > 0) {
				String endTime = lines.get(index - 1).substring(12, 33).split(",")[1];
				String startTime = lines.get(index).substring(12, 33).split(",")[0];
				if (endTime.compareTo(startTime) > 0) {
					System.err.println("[时间轴顺序异常] " + lines.get(index));
				}
			}
			int number = lines.get(index).split("- ").length;
			if ((lines.get(index).contains(",,-") || lines.get(index).contains("}-")) && number != 5 && number != 7 && number != 9) {
				if (!lines.contains(SEPARATOR) && number == 3) {
					continue;
				}
				System.err.println("[对话结构异常] " + lines.get(index));
			}
			if (lines.get(index).contains("")) {
				System.err.println("[特殊字符] " + lines.get(index));
			}
		}
		return lines;
	}

	private static String build(List<String> lines) throws Exception {
		StringBuilder builder = new StringBuilder();
		builder.append("[Script Info]").append(ENTER);
		builder.append("PlayResX: 1280").append(ENTER);
		builder.append("PlayResY: 720").append(ENTER).append(ENTER);
		builder.append("[V4+ Styles]").append(ENTER);
		builder.append("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding").append(ENTER);
		builder.append("Style: Default,SimHei,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,10,1").append(ENTER);
		builder.append("Style: Title,SimHei,35,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,30,1").append(ENTER);
		builder.append("Style: Note,SimHei,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,40,1").append(ENTER);
		builder.append(ENTER);
		builder.append("[Events]").append(ENTER);
		builder.append("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text").append(ENTER);
		List<String> titles = new ArrayList<>();
		List<String> notes = new ArrayList<>();
		List<String> subtitles = new ArrayList<>();
		for (String line : lines) {
			if (line.startsWith("Dialogue")) {
				if (line.contains(SEPARATOR)) {
					subtitles.add(line);
				} else if (line.contains(",,《")) {
					titles.add(line.replace(",Default,", ",Title,").replaceAll("[:：]( )*", "："));
				} else {
					notes.add(line.replace(",Default,", ",Note,"));
				}
			}
		}
		notes.addAll(titles);
		for (String line : notes) {
			line = sinicized(filter(line));
			String[] array = line.split(",,");
			line = array[0].replace("：", ":").replace("Dialogue:", "Dialogue: ").replace("  ", " ") + ",," + array[1];
			if (line.contains("(")) {
				line = line.replace("，", " ").replace("： ", "：").trim();
			}
			builder.append(line).append(ENTER);
		}
		for (String line : subtitles) {
			builder.append(sinicized(filter(line))).append(ENTER);
		}
		return builder.toString();
	}

	private static String filter(String line) {
		line = line.replace("{\\r}", "").replace(",NTP,", ",,").replace(",!Effect,", ",,").replace(",0000,0000,0000,", ",,,,").replaceAll("\\{\\\\blur[0-9]*\\}", "").replaceAll("\\{\\\\an[0-9]*\\}", "").replaceAll("\\{\\\\fs[0-9]*\\}", "");
		line = line.replace("『", "“").replace("』", "”").replace("「", "“").replace("」", "”");
		line = line.replace("（", "(").replace("）", ")").replace("！", "!").replace("？", "?").replace("，", ", ").replace(";", "；").replace("；", ",").replace("。", " ").replace("　", " ").replace("–", "-").replace("…", "...").replace("--", "...").replace("''", "\"").replace("", "");
		line = line.replace(",,...", ",,").replace("}...", "}").replace("- ", "-").replace("?-", "? -").replace("!-", "! -").replace(".-", ". -").replace("- ...", "- ").replace("-...", "- ").replace(",,-", ",,- ").replace("}-", "}- ").replace(" -", " - ").replace(".\"", "\".");
		line = line.replace("?", "? ").replace("!", "! ").replace("( ", "(").replace(" )", ")").replace("(-", "(- ").replace(" .", ".").replace(" ,", ",").replace(" !", "!").replace(" ?", "?");
		line = line.replace("lt'", "It'").replace(" lt", " It").replace("lsn'", "Isn'").replace(" ls ", " Is ").replace(" l ", " I ").replace(",,l ", ",,I ").replace("}l ", "}I ").replace("\"l ", "\"I ").replace(" i ", " I ").replace(",,i ", ",,I ").replace("}i ", "}I ").replace("\"i ", "\"I ");
		line = line.replace("牠", "它").replace("麽", "么").replace("．．．", "...");
		if (line.endsWith("-")) {
			line = line.substring(0, line.length() - 2) + "...";
		}
		line = line.replaceAll("(\\.){3,}", "...").replaceAll(",,( )+", ",,").replaceAll("}( )+", "}").replaceAll("( )+", " ").replaceAll("((\\.)+!|!(\\.)+)", "!").replaceAll("((\\.)+\\?|\\?(\\.)+)", "?").trim();
		line = line.replace(".\"..", "...\"").replace(" said\", ", " said, \"").replace("] ", "]").replace("O.K.", "OK");
		line = line.replace("ａ", "a");
		line = line.replace("ｂ", "b");
		line = line.replace("ｃ", "c");
		line = line.replace("ｄ", "d");
		line = line.replace("ｅ", "e");
		line = line.replace("ｆ", "f");
		line = line.replace("ｇ", "g");
		line = line.replace("ｈ", "h");
		line = line.replace("ｉ", "i");
		line = line.replace("ｇ", "g");
		line = line.replace("ｋ", "k");
		line = line.replace("ｌ", "l");
		line = line.replace("ｍ", "m");
		line = line.replace("ｎ", "n");
		line = line.replace("ｏ", "o");
		line = line.replace("ｐ", "p");
		line = line.replace("ｑ", "q");
		line = line.replace("ｒ", "r");
		line = line.replace("ｓ", "s");
		line = line.replace("ｔ", "t");
		line = line.replace("ｕ", "u");
		line = line.replace("ｖ", "v");
		line = line.replace("ｗ", "w");
		line = line.replace("ｘ", "x");
		line = line.replace("ｙ", "y");
		line = line.replace("ｚ", "z");
		line = line.replace("Ａ", "A");
		line = line.replace("Ｂ", "B");
		line = line.replace("Ｃ", "C");
		line = line.replace("Ｄ", "D");
		line = line.replace("Ｅ", "E");
		line = line.replace("Ｆ", "F");
		line = line.replace("Ｇ", "G");
		line = line.replace("Ｈ", "H");
		line = line.replace("Ｉ", "I");
		line = line.replace("Ｊ", "J");
		line = line.replace("Ｋ", "K");
		line = line.replace("Ｌ", "L");
		line = line.replace("Ｍ", "M");
		line = line.replace("Ｎ", "N");
		line = line.replace("Ｏ", "O");
		line = line.replace("Ｐ", "P");
		line = line.replace("Ｑ", "Q");
		line = line.replace("Ｒ", "R");
		line = line.replace("Ｓ", "S");
		line = line.replace("Ｔ", "T");
		line = line.replace("Ｕ", "U");
		line = line.replace("Ｖ", "V");
		line = line.replace("Ｗ", "W");
		line = line.replace("Ｘ", "X");
		line = line.replace("Ｙ", "Y");
		line = line.replace("Ｚ", "Z");
		line = line.replace("１", "1");
		line = line.replace("２", "2");
		line = line.replace("３", "3");
		line = line.replace("４", "4");
		line = line.replace("５", "5");
		line = line.replace("６", "6");
		line = line.replace("７", "7");
		line = line.replace("８", "8");
		line = line.replace("９", "9");
		line = line.replace("０", "0");
		String[] array = line.split(SEPARATOR_REGEX);
		return array[0].trim() + (array.length == 1 ? "" : SEPARATOR + array[1].trim());
	}

	private static String sinicized(String line) {
		String[] array1 = line.split(SEPARATOR_REGEX);
		String[] array2 = array1[0].split(",,");
		String english = array1.length == 1 ? "" : SEPARATOR + array1[1];
		String chinese = array2[1].replace("...", "…").replace(". ", "，");
		chinese = chinese.replaceAll(",( )*", "，").replaceAll("!( )*", "！").replaceAll("\\?( )*", "？").replaceAll(":( )*", "：");
		if (!line.contains("∮") && !chinese.startsWith("《")) {
			chinese = chinese.replace(" ", "，");
		}
		chinese = chinese.replace("，-", " -").replace("-，", "- ");
		boolean status = true;
		if (chinese.contains(("\""))) {
			for (char c : chinese.toCharArray()) {
				if (String.valueOf(c).equals("\"")) {
					chinese = chinese.replaceFirst("(\")", status ? "“" : "”");
					status = !status;
				}
			}
			if (chinese.endsWith("“")) {
				chinese = chinese.substring(0, chinese.length() - 1) + "”";
			}
		}
		if (english.endsWith("? \"") || english.endsWith("! \"") || english.endsWith(". \"") || english.endsWith(", \"")) {
			english = english.substring(0, english.length() - 2) + "\"";
		}
		if (!Pattern.compile("[0-9]+\\.[0-9]+").matcher(chinese).find()) {
			chinese = chinese.replace(".", "，");
		}
		chinese = chinese.replace("“，", "”，").replace("，”", "”").replace("…，", "…").replace("“…", "“").replace("？-", "？ -").replace("！-", "！ -").replace("说，“", "说：“").replace("道，“", "道：“").replace("喊，“", "喊：“").replace("写，“", "写：“").replace("想，“", "想：“");
		english = english.replace(".\"", "\".").replace(",\"", "\",").replace(",", ", ").replace("  ", " ").replace("..\".", "...\"");
		if (english.length() > 1 && english.substring(english.length() - 1).matches("[a-zA-Z0-9]")) {
			english += ".";
		}
		if (chinese.endsWith("吗")) {
			chinese += "？";
		}
		line = array2[0] + ",," + (chinese + english).replace("“\\N", "”\\N").replace("，”\\N", "”\\N");
		if (!status) {
			// System.err.println(line);
		}
		line = line.replace("∮，", "∮ ").replace("，∮", " ∮").replace("： ", "：").replace("，\\N{", "\\N{").replace("！！", "！");
		return line.trim();
	}

	private static void write(String path, String content) throws Exception {
		try (FileWriter writer = new FileWriter(path)) {
			writer.write(content);
			writer.flush();
		}
	}

	private static boolean hasChinese(String text) {
		for (char c : text.toCharArray()) {
			if (String.valueOf(c).matches("[\u4e00-\u9fa5]")) {
				return true;
			}
		}
		return false;
	}

	private static boolean isLowerCase(char c) {
		return c >= 97 && c <= 122;
	}
}
