import java.lang.reflect.*;
import java.util.*;
public class Generate {
  static String t(Type t) { return t.getTypeName().replace('$','.'); }
  public static void main(String[] args) throws Exception {
    Class<?> c=Class.forName(args[0]);
    System.out.println("package dev.ferrite.server;\n@SuppressWarnings({\"deprecation\",\"unchecked\",\"rawtypes\"})\nabstract class ServerStubs implements "+c.getCanonicalName()+" {");
    Map<String,Method> ms=new TreeMap<>();
    for(Method m:c.getMethods()) if(Modifier.isAbstract(m.getModifiers())) {
      String k=m.getName()+Arrays.toString(m.getParameterTypes()); Method old=ms.get(k);
      if(old==null || old.getReturnType().isAssignableFrom(m.getReturnType())) ms.put(k,m);
    }
    for(Method m:ms.values()) {
      StringBuilder b=new StringBuilder("public ");
      TypeVariable<Method>[] vars=m.getTypeParameters();
      if(vars.length>0) { b.append("<"); for(int i=0;i<vars.length;i++) { if(i>0)b.append(", "); b.append(vars[i].getName()); Type[] bounds=vars[i].getBounds(); if(!(bounds.length==1&&bounds[0]==Object.class)){b.append(" extends "); for(int j=0;j<bounds.length;j++){if(j>0)b.append(" & ");b.append(t(bounds[j]));}} } b.append("> "); }
      b.append(t(m.getGenericReturnType())).append(" ").append(m.getName()).append("(");
      Type[] ps=m.getGenericParameterTypes(); for(int i=0;i<ps.length;i++){if(i>0)b.append(", ");b.append(t(ps[i])).append(" a").append(i);} b.append(") { throw new UnsupportedOperationException(\"Ferrite: not implemented: Server#").append(m.getName()).append("\"); }"); System.out.println(b);
    }
    System.out.println("}");
  }
}
