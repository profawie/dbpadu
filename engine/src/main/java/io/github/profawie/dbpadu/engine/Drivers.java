package io.github.profawie.dbpadu.engine;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.lang.reflect.Modifier;
import java.net.URISyntaxException;
import java.net.URL;
import java.net.URLClassLoader;
import java.sql.Connection;
import java.sql.Driver;
import java.sql.DriverManager;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Enumeration;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Properties;
import java.util.ServiceConfigurationError;
import java.util.ServiceLoader;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.jar.JarEntry;
import java.util.jar.JarFile;
import java.util.jar.Manifest;

/**
 * The JDBC drivers: the ones inside the engine, and the drivers the user
 * added, each from jar files of its own.
 */
final class Drivers {
    private Drivers() {}

    /** The class loader over one set of the user's jar files. */
    private record Library(URLClassLoader loader, List<File> jars) {}

    /** A driver from the user's own jars, loaded once and kept. */
    private record Loaded(Driver driver, String version) {}

    private static final Map<String, Library> LIBRARIES = new ConcurrentHashMap<>();
    private static final Map<String, Loaded> OWN = new ConcurrentHashMap<>();

    /** Name, package, class and version of every driver inside the engine. */
    static JsonArray bundled() {
        Properties properties = new Properties();
        try (InputStream in = Drivers.class.getResourceAsStream("/drivers.properties")) {
            if (in != null) {
                properties.load(in);
            }
        } catch (IOException unreadable) {
            // The list stays empty; the drivers themselves still work.
        }
        JsonArray drivers = new JsonArray();
        for (String kind : properties.getProperty("kinds", "").split(",")) {
            if (kind.isBlank()) {
                continue;
            }
            JsonObject driver = new JsonObject();
            driver.addProperty("kind", kind);
            driver.addProperty("name", properties.getProperty(kind + ".name", ""));
            driver.addProperty("package", properties.getProperty(kind + ".package", ""));
            driver.addProperty("className", properties.getProperty(kind + ".class", ""));
            driver.addProperty("version", properties.getProperty(kind + ".version", ""));
            drivers.add(driver);
        }
        return drivers;
    }

    /**
     * Open a connection. With {@code libraries}, the driver comes from those
     * jar files (or folders of jar files) instead of the engine;
     * {@code className} may be empty to use the driver the jars announce.
     */
    static Connection connect(
            String url, Properties properties, List<String> libraries, String className)
            throws SQLException {
        if (libraries.isEmpty()) {
            return DriverManager.getConnection(url, properties);
        }
        Driver driver = own(libraries, className).driver();
        Connection connection;
        // Some drivers look for their own classes through the thread.
        Thread thread = Thread.currentThread();
        ClassLoader before = thread.getContextClassLoader();
        thread.setContextClassLoader(driver.getClass().getClassLoader());
        try {
            connection = driver.connect(url, properties);
        } finally {
            thread.setContextClassLoader(before);
        }
        if (connection == null) {
            throw new SQLException(
                "The driver " + driver.getClass().getName()
                    + " does not understand the address " + url
                    + ". Check the URL template of the driver.");
        }
        return connection;
    }

    /**
     * What is in the user's jars: every driver class found, and the class
     * and version of the one that would be used. Without a usable driver
     * the classes are still listed, with the reason under {@code error}.
     */
    static JsonObject describe(List<String> libraries, String className) throws SQLException {
        Library library = library(libraries);
        JsonArray classes = new JsonArray();
        for (String name : driverClasses(library)) {
            classes.add(name);
        }
        JsonObject result = new JsonObject();
        result.add("classes", classes);
        try {
            Loaded loaded = own(libraries, className);
            result.addProperty("className", loaded.driver().getClass().getName());
            result.addProperty("version", loaded.version());
        } catch (SQLException problem) {
            if (classes.isEmpty()) {
                throw problem;
            }
            result.addProperty("error", problem.getMessage());
        }
        return result;
    }

    /** The jar files named, with the jar files of the folders named. */
    private static List<File> jars(List<String> libraries) throws SQLException {
        List<File> jars = new ArrayList<>();
        for (String path : libraries) {
            File file = new File(path);
            if (file.isDirectory()) {
                File[] inside = file.listFiles(
                    (dir, name) -> name.toLowerCase(Locale.ROOT).endsWith(".jar"));
                if (inside != null) {
                    Arrays.sort(inside);
                    jars.addAll(Arrays.asList(inside));
                }
            } else if (file.isFile()) {
                jars.add(file);
            } else {
                throw new SQLException("The driver file was not found: " + path);
            }
        }
        if (jars.isEmpty()) {
            throw new SQLException("There is no .jar file in " + String.join(", ", libraries));
        }
        return jars;
    }

    /**
     * One class loader per set of jar files, so two versions of a driver,
     * such as MySQL 5 and MySQL 8, are loaded side by side and never see
     * each other's classes.
     */
    private static Library library(List<String> libraries) throws SQLException {
        List<File> jars = jars(libraries);
        StringBuilder key = new StringBuilder();
        for (File jar : jars) {
            // A replaced file is a new driver.
            key.append(jar.getAbsolutePath()).append('|').append(jar.lastModified()).append('\n');
        }
        Library known = LIBRARIES.get(key.toString());
        if (known != null) {
            return known;
        }
        URL[] urls = new URL[jars.size()];
        try {
            for (int i = 0; i < urls.length; i++) {
                urls[i] = jars.get(i).toURI().toURL();
            }
        } catch (IOException problem) {
            throw new SQLException("The driver files could not be read: " + problem);
        }
        // Only Java itself is shared with the jars, so their classes win
        // over the driver of the same name inside the engine.
        Library library = new Library(
            new URLClassLoader(urls, ClassLoader.getPlatformClassLoader()), jars);
        Library raced = LIBRARIES.putIfAbsent(key.toString(), library);
        return raced != null ? raced : library;
    }

    /**
     * The driver classes in the jars: the ones they announce, and for jars
     * from before JDBC 4, which announce nothing, the classes named
     * {@code ...Driver} that are JDBC drivers.
     */
    private static List<String> driverClasses(Library library) {
        Set<String> names = new LinkedHashSet<>();
        try {
            Iterator<Driver> announced = ServiceLoader.load(Driver.class, library.loader()).iterator();
            while (announced.hasNext()) {
                try {
                    Driver found = announced.next();
                    if (found.getClass().getClassLoader() == library.loader()) {
                        names.add(found.getClass().getName());
                    }
                } catch (ServiceConfigurationError | LinkageError broken) {
                    // One driver that does not load does not hide the others.
                }
            }
        } catch (ServiceConfigurationError | LinkageError broken) {
            // Look at the class names instead.
        }
        for (File file : library.jars()) {
            try (JarFile jar = new JarFile(file)) {
                Enumeration<JarEntry> entries = jar.entries();
                while (entries.hasMoreElements()) {
                    String entry = entries.nextElement().getName();
                    if (!entry.endsWith("Driver.class") || entry.contains("$")
                            || entry.startsWith("META-INF/")) {
                        continue;
                    }
                    String name = entry.substring(0, entry.length() - ".class".length())
                        .replace('/', '.');
                    try {
                        Class<?> type = Class.forName(name, false, library.loader());
                        if (Driver.class.isAssignableFrom(type) && !type.isInterface()
                                && !Modifier.isAbstract(type.getModifiers())
                                && Modifier.isPublic(type.getModifiers())) {
                            names.add(name);
                        }
                    } catch (ClassNotFoundException | LinkageError notUsable) {
                        // Not a class this Java can load: not a driver to offer.
                    }
                }
            } catch (IOException unreadable) {
                // What the jar announces is still listed.
            }
        }
        return new ArrayList<>(names);
    }

    private static Loaded own(List<String> libraries, String className) throws SQLException {
        Library library = library(libraries);
        String wanted = className == null ? "" : className.trim();
        String where = library.jars().size() == 1
            ? library.jars().get(0).getPath()
            : "the driver's " + library.jars().size() + " files";
        if (wanted.isEmpty()) {
            List<String> found = driverClasses(library);
            if (found.isEmpty()) {
                throw new SQLException(
                    "No JDBC driver was found in " + where + ". Enter the driver class.");
            }
            wanted = found.get(0);
        }
        String key = System.identityHashCode(library.loader()) + "|" + wanted;
        Loaded known = OWN.get(key);
        if (known != null) {
            return known;
        }
        try {
            Driver driver = (Driver) Class.forName(wanted, true, library.loader())
                .getDeclaredConstructor()
                .newInstance();
            Loaded loaded = new Loaded(driver, version(driver));
            OWN.put(key, loaded);
            return loaded;
        } catch (ClassNotFoundException missing) {
            throw new SQLException("The class " + wanted + " is not in " + where);
        } catch (ClassCastException notDriver) {
            throw new SQLException("The class " + wanted + " is not a JDBC driver.");
        } catch (ReflectiveOperationException | LinkageError problem) {
            throw new SQLException("The driver in " + where + " could not be loaded: " + problem);
        }
    }

    /** The version the driver's jar states, else the driver's own major.minor. */
    private static String version(Driver driver) {
        try {
            File file = new File(
                driver.getClass().getProtectionDomain().getCodeSource().getLocation().toURI());
            try (JarFile jar = new JarFile(file)) {
                Manifest manifest = jar.getManifest();
                if (manifest != null) {
                    for (String name : new String[] {"Implementation-Version", "Bundle-Version"}) {
                        String value = manifest.getMainAttributes().getValue(name);
                        if (value != null && !value.isBlank()) {
                            return value.trim();
                        }
                    }
                }
            }
        } catch (IOException | URISyntaxException | RuntimeException unreadable) {
            // Fall through to what the driver says.
        }
        return driver.getMajorVersion() + "." + driver.getMinorVersion();
    }
}
