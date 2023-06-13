package bollard;

import io.swagger.codegen.*;
import io.swagger.codegen.languages.RustServerCodegen;
import io.swagger.models.properties.*;
import io.swagger.models.parameters.Parameter;
import io.swagger.models.parameters.SerializableParameter;
import io.swagger.models.parameters.BodyParameter;
import io.swagger.util.Json;

import io.swagger.models.*;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.apache.commons.lang3.StringUtils;

import java.util.*;
import java.util.Map.Entry;

public class BollardCodegen extends RustServerCodegen {
    private static final Logger LOGGER = LoggerFactory.getLogger(BollardCodegen.class);

    public BollardCodegen() {
        super();
        typeMapping.put("DateTime", "BollardDate");
    }

    // Declare custom additions to inline enums that are behaving differently
    // than the official spec
    private static HashMap<String, List<Map<String, String>>> patchEnumValues;
    static {
        patchEnumValues = new HashMap<String, List<Map<String, String>>>();
        Map<String, String> additionalEnumValues = new HashMap<String, String>();
        List<Map<String, String>> enumValues = new ArrayList<Map<String, String>>();

        additionalEnumValues.put("name", "ROLLBACK_STARTED");
        additionalEnumValues.put("value", "\"rollback_started\"");
        enumValues.add(additionalEnumValues);

        additionalEnumValues = new HashMap<String, String>();
        additionalEnumValues.put("name", "ROLLBACK_PAUSED");
        additionalEnumValues.put("value", "\"rollback_paused\"");
        enumValues.add(additionalEnumValues);

        additionalEnumValues = new HashMap<String, String>();
        additionalEnumValues.put("name", "ROLLBACK_COMPLETED");
        additionalEnumValues.put("value", "\"rollback_completed\"");
        enumValues.add(additionalEnumValues);

        patchEnumValues.put("ServiceUpdateStatusStateEnum", enumValues);
    }

    private static ArrayList<String> enumToString;
    static {
        enumToString = new ArrayList();
        enumToString.add("HostConfigLogConfig");
    }

    private static ArrayList<String> upperCaseModelFields;
    static {
        upperCaseModelFields = new ArrayList();
        upperCaseModelFields.add("IdResponse");
    }

    @Override
    public void preprocessSwagger(Swagger swagger) {
        Info info = swagger.getInfo();
        List versionComponents = new ArrayList();
        versionComponents.add((String) additionalProperties.get(CodegenConstants.PACKAGE_VERSION));

        info.setVersion(StringUtils.join(versionComponents, "."));

        super.preprocessSwagger(swagger);
    }

    @Override
    public String getTypeDeclaration(Property p) {
        String type = super.getTypeDeclaration(p);

        // This is a "fallback" type, and allows some parts of the Docker API
        // that receive an empty JSON '{}' value.
        if ("object".equals(type)) {
            type = "HashMap<(), ()>";
        }

        return type;
    }

    @Override
    public CodegenProperty fromProperty(String name, Property p) {
        CodegenProperty property = super.fromProperty(name, p);

        // Remove extraneous references
        if (property.datatype.startsWith("models::")) {
            property.datatype = property.datatype.replace("models::", "");
        }

        return property;
    }

    @Override
    public Map<String, Object> postProcessAllModels(Map<String, Object> objs) {
        Map<String, Object> newObjs = super.postProcessAllModels(objs);

        // Index all CodegenModels by model name.
        HashMap<String, CodegenModel> allModels = new HashMap<String, CodegenModel>();
        for (Entry<String, Object> entry : objs.entrySet()) {
            String modelName = toModelName(entry.getKey());
            Map<String, Object> inner = (Map<String, Object>) entry.getValue();
            List<Map<String, Object>> models = (List<Map<String, Object>>) inner.get("models");
            for (Map<String, Object> mo : models) {
                CodegenModel cm = (CodegenModel) mo.get("model");
                allModels.put(modelName, cm);
            }
        }

        for (Entry<String, CodegenModel> entry : allModels.entrySet()) {
            CodegenModel model = entry.getValue();

            if (upperCaseModelFields.contains(model.classname)) {
                model.vendorExtensions.put("x-rustgen-upper-case", true);
            }

            // Special case for numeric Enums
            if (model.isEnum && model.dataType != null && (model.dataType.equals("i8") || model.dataType.equals("i16") || model.dataType.equals("i32") || model.dataType.equals("i64"))) {
                model.vendorExtensions.put("x-rustgen-numeric-enum", true);
                ArrayList<HashMap<String, String>> lst = (ArrayList) model.allowableValues.get("enumVars");
                for (HashMap<String, String> enumVar : lst) {
                    String val = enumVar.get("value");
                    enumVar.put("value", val.replace("\"", ""));
                }
            }

            for (CodegenProperty prop : model.vars) {
                if (prop.name.contains("i_pv6")) {
                    prop.name = prop.name.replace("i_pv6", "ipv6");
                } else if (prop.name.contains("i_pv4")) {
                    prop.name = prop.name.replace("i_pv4", "ipv4");
                } else if (prop.name.contains("_i_ops")) {
                    prop.name = prop.name.replace("_i_ops", "_iops");
                } else if (prop.name.contains("_i_ds")) {
                    prop.name = prop.name.replace("_i_ds", "_ids");
                } else if (prop.name.contains("_c_as")) {
                    prop.name = prop.name.replace("_c_as", "_cas");
                } else if (prop.name.equals("_type")) {
                    prop.name = "typ";
                }

                if (prop.name.equals("aux") && model.classname.equals("BuildInfo")) {
                    prop.vendorExtensions.put("x-rustgen-grpc-aux", true);
                    model.vendorExtensions.put("x-rustgen-grpc-aux", true);
                }

                if (upperCaseModelFields.contains(model.classname)) {
                    prop.vendorExtensions.put("x-rustgen-upper-case", true);
                }

                if (prop.dataFormat != null && (prop.dataFormat.equals("dateTime") || prop.datatype.equals("BollardDate"))) {
                    // set DateTime format on properties where appropriate
                    prop.vendorExtensions.put("x-rustgen-is-datetime", true);
                    prop.datatype = "BollardDate";
                }
                if (prop.isEnum) {
                    if (enumToString.contains(model.classname)) {
                        prop.isEnum = false;
                    }
                    ArrayList<HashMap<String, String>> vars = (ArrayList<HashMap<String, String>>) prop.allowableValues
                            .get("enumVars");
                    for (HashMap<String, String> enumVar : vars) {
                        String enumValue = enumVar.get("value");

                        // ensure we can deserialize inline enum values encoded as empty strings
                        if (enumValue != null && enumValue.length() <= 2) {
                            prop.vendorExtensions.put("x-rustgen-has-empty-enum", true);
                        }
                    }

                    // add additional enum values that get patched in at the template level
                    if (patchEnumValues.containsKey(model.classname + prop.enumName)) {
                        prop.vendorExtensions.put("x-rustgen-additional-enum-values",
                                patchEnumValues.get(model.classname + prop.enumName));
                    }
                }
            }
        }

        return newObjs;
    }

    @Override
    public void postProcessModelProperty(CodegenModel model, CodegenProperty property) {
        super.postProcessModelProperty(model, property);

        if (property.datatype.equals("isize")) {
            // needed for windows
            property.datatype = "i64";
        }

        if (property.dataFormat != null) {
            switch (property.dataFormat) {
                case "uint64":
                    property.datatype = "u64";
                    break;
                case "int64":
                    property.datatype = "i64";
                    break;
                case "uint32":
                    property.datatype = "u32";
                    break;
                case "int32":
                    property.datatype = "i32";
                    break;
                case "uint16":
                    property.datatype = "u16";
                    break;
                case "int16":
                    property.datatype = "i16";
                    break;
                case "uint8":
                    property.datatype = "u8";
                    break;
                case "int8":
                    property.datatype = "i8";
                    break;
            }
        }
    }

    @Override
    public String toEnumVarName(String value, String datatype) {
        String name = super.toEnumVarName(value, datatype);
        if (name.length() == 0) {
            return "EMPTY";
        }
        return name;
    }
}
