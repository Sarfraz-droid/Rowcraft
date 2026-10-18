import java.io.*;
import java.sql.*;
import java.util.*;
import java.nio.charset.StandardCharsets;

// Small line protocol; credentials travel over stdin, never the process command line.
class Bridge {
    static final PrintStream protocol = System.out;
    static String decode(String s) { return new String(Base64.getDecoder().decode(s), StandardCharsets.UTF_8); }
    static String quote(String s) {
        if (s == null) return "null";
        StringBuilder b = new StringBuilder("\"");
        for (char c : s.toCharArray()) {
            switch(c) {
                case '"': b.append("\\\""); break;
                case '\\': b.append("\\\\"); break;
                case '\n': b.append("\\n"); break;
                case '\r': b.append("\\r"); break;
                case '\t': b.append("\\t"); break;
                default: if(c < 32) b.append(String.format("\\u%04x", (int)c)); else b.append(c);
            }
        }
        return b.append('"').toString();
    }
    static void send(String s) { protocol.println("DG:" + s); protocol.flush(); }
    static String cell(ResultSet r, int i) throws SQLException {
        Object o = r.getObject(i);
        if (o == null) return "null";
        if (o instanceof Boolean) return o.toString();
        if (o instanceof Byte || o instanceof Short || o instanceof Integer || o instanceof Long) return o.toString();
        if (o instanceof Float || o instanceof Double) { double d = ((Number)o).doubleValue(); if(Double.isFinite(d)) return o.toString(); }
        if (o instanceof byte[]) return "{\"bytes\":" + quote(Base64.getEncoder().encodeToString((byte[])o)) + "}";
        return quote(r.getString(i));
    }
    static String result(ResultSet r, int limit) throws SQLException {
        ResultSetMetaData m = r.getMetaData();
        StringBuilder s = new StringBuilder("{\"columns\":[");
        for(int i=1;i<=m.getColumnCount();i++) { if(i>1)s.append(','); s.append(quote(m.getColumnLabel(i))); }
        s.append("],\"rows\":["); int count=0;
        boolean available = r.next();
        while(count<limit && available) {
            if(count++>0)s.append(','); s.append('[');
            for(int i=1;i<=m.getColumnCount();i++) { if(i>1)s.append(','); s.append(cell(r,i)); }
            s.append(']'); available = r.next();
        }
        boolean truncated = available;
        return s.append("],\"truncated\":").append(truncated).append('}').toString();
    }
    static String metadata(Connection c) throws SQLException {
        DatabaseMetaData m = c.getMetaData();
        StringBuilder s = new StringBuilder("{\"columns\":[\"schema\",\"table\",\"kind\",\"column\",\"type\",\"nullable\",\"default\",\"ordinal\"],\"rows\":[");
        String defaultSchema=null;
        try { defaultSchema=c.getSchema(); } catch(SQLException | AbstractMethodError ignored) {}
        if(defaultSchema==null && m.getDatabaseProductName().equalsIgnoreCase("SQLite"))defaultSchema="main";
        s.append("[").append(quote(c.getCatalog())).append(',').append(quote(defaultSchema)).append(",\"DG_DATABASE\",null,null,true,null,0]");
        boolean first=false;
        try(ResultSet t=m.getTables(c.getCatalog(),null,"%",new String[]{"TABLE","VIEW","MATERIALIZED VIEW"})) {
            while(t.next()) {
                String schema=t.getString("TABLE_SCHEM"), table=t.getString("TABLE_NAME"), kind=t.getString("TABLE_TYPE");
                if(schema == null) schema="main";
                if(schema.equalsIgnoreCase("information_schema") || schema.equalsIgnoreCase("pg_catalog") || schema.equalsIgnoreCase("sys")) continue;
                try(ResultSet cols=m.getColumns(c.getCatalog(),escapePattern(m,t.getString("TABLE_SCHEM")),escapePattern(m,table),"%")) {
                    boolean any=false;
                    while(cols.next()) {
                        if(!first)s.append(','); first=false; any=true;
                        s.append('[').append(quote(schema)).append(',').append(quote(table)).append(',').append(quote(kind)).append(',').append(quote(cols.getString("COLUMN_NAME"))).append(',').append(quote(cols.getString("TYPE_NAME"))).append(',').append(cols.getInt("NULLABLE") != DatabaseMetaData.columnNoNulls).append(',').append(quote(cols.getString("COLUMN_DEF"))).append(',').append(cols.getInt("ORDINAL_POSITION")).append(']');
                    }
                    if(!any) { if(!first)s.append(',');first=false;s.append('[').append(quote(schema)).append(',').append(quote(table)).append(',').append(quote(kind)).append(",null,null,true,null,0]"); }
                }
            }
        }
        return s.append("]}").toString();
    }
    static String escapePattern(DatabaseMetaData m, String name) throws SQLException {
        if(name==null)return null;
        String escape=m.getSearchStringEscape();
        if(escape==null || escape.isEmpty())return name;
        return name.replace(escape,escape+escape).replace("_",escape+"_").replace("%",escape+"%");
    }
    public static void main(String[] args) {
        System.setOut(System.err); // Driver logging cannot corrupt the protocol.
        try(BufferedReader in=new BufferedReader(new InputStreamReader(System.in,StandardCharsets.UTF_8))) {
            String[] init=in.readLine().split("\t",-1);
            String cls=decode(init[0]); if(!cls.isEmpty()) Class.forName(cls);
            Properties p=new Properties();
            for(int i=2;i+1<init.length;i+=2)p.setProperty(decode(init[i]),decode(init[i+1]));
            boolean readOnly=Boolean.parseBoolean((String)p.remove("_dglite.readOnly"));
            boolean manual=Boolean.parseBoolean((String)p.remove("_dglite.manual"));
            String isolation=(String)p.remove("_dglite.isolation");
            DriverManager.setLoginTimeout(30);
            try(Connection c=DriverManager.getConnection(decode(init[1]),p)) {
                if(readOnly)c.setReadOnly(true);
                if(manual)c.setAutoCommit(false);
                if(isolation!=null && !isolation.isEmpty())c.setTransactionIsolation(Integer.parseInt(isolation));
                send("{\"ok\":true}");
                String line;
                while((line=in.readLine())!=null) {
                    try {
                        String[] req=line.split("\t",-1); String sql=decode(req[1]);
                        if(req[0].equals("M")) { send(metadata(c)); continue; }
                        int limit=Integer.parseInt(req[2]);
                        try(Statement st=c.createStatement()) {
                            st.setQueryTimeout(120);
                            if(limit < Integer.MAX_VALUE)st.setMaxRows(limit+1);
                            boolean has=st.execute(sql); String last="{\"columns\":[],\"rows\":[]}";
                            while(true) {
                                if(has) { try(ResultSet r=st.getResultSet()){last=result(r,limit);} }
                                else { long n=st.getLargeUpdateCount(); if(n<0)break; last="{\"columns\":[],\"rows\":[],\"affected\":"+n+"}"; }
                                has=st.getMoreResults();
                            }
                            send(last);
                        }
                    } catch(Exception e) { send("{\"error\":"+quote(e.getMessage())+"}"); }
                }
            }
        } catch(Exception e) { send("{\"error\":"+quote(e.getMessage())+"}"); }
    }
}
