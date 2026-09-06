import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.util.Arrays;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;

import javax.imageio.ImageIO;

public class ComicRename {
	public static final String NAME = "ÎÚÁúÅÉ³öËù";

	public static void main(String[] args) throws Exception {
		for (File file : new File("G:\\¶¯Âþ\\Âþ»­\\" + NAME).listFiles()) {
			if (file.isDirectory()) {
				String[] files = file.list();
				Arrays.sort(files);
				int index = 1;
				for (String fileName : files) {
					if (fileName.equals("Thumbs.db")) {
						new File(file.getAbsolutePath() + "\\" + "Thumbs.db").delete();
						continue;
					}
					fileName = fileName.toLowerCase();
					String pageNo = "";
					if (index < 10) {
						pageNo = "00" + index;
					} else if (index < 100) {
						pageNo = "0" + index;
					} else {
						pageNo = "" + index;
					}
					StringBuilder builder = new StringBuilder();
					builder.append(file.getName().substring(4));
					builder.append("_");
					builder.append(pageNo);
					String path = file.getPath() + "\\";
					File input = new File(path + builder.toString() + ".tmp");
					new File(path + fileName).renameTo(input);
					File output = new File(path + builder.toString() + ".jpg");
					System.out.println(input.getName() + " -> " + output.getName());
					ImageIO.write(ImageIO.read(input), "jpg", output);
					input.delete();
					index++;
				}
				zipFiles(file, new File(file.getAbsolutePath().replace(".", "_") + ".zip"));
				System.out.println(file.getName() + " is OK.");
			}
		}
		System.out.println("Done!");
	}

	private static void zipFiles(File directory, File zipFile) throws Exception {
		FileOutputStream fileOutputStream = new FileOutputStream(zipFile);
		ZipOutputStream zipOutputStream = new ZipOutputStream(fileOutputStream);
		FileInputStream fileInputStream = null;
		for (String fileName : directory.list()) {
			File srcFile = new File(directory.getPath() + "\\" + fileName);
			fileInputStream = new FileInputStream(srcFile);
			zipOutputStream.putNextEntry(new ZipEntry(srcFile.getName()));
			int len;
			byte[] buffer = new byte[1024];
			while ((len = fileInputStream.read(buffer)) > 0) {
				zipOutputStream.write(buffer, 0, len);
			}
		}
		zipOutputStream.closeEntry();
		zipOutputStream.close();
		fileInputStream.close();
		fileOutputStream.close();
	}
}
