# frozen_string_literal: true

module Inkoc
  # Compiler for translating Inko source files into IVM bytecode.
  class Compiler
    PASSES = [
      Pass::PathToSource,
      Pass::SourceToAst,
      Pass::DesugarObject,
      Pass::DesugarMethod,
      Pass::DesugarLoops,
      Pass::DefineModuleType,
      Pass::TrackModule,
      Pass::InsertImplicitImports,
      Pass::CollectImports,
      Pass::AddImplicitImportSymbols,
      Pass::CompileImportedModules,
      Pass::SetupSymbolTables,
      Pass::DefineThisModuleType,
      Pass::DefineImportTypes,
      Pass::DefineTypeSignatures,
      Pass::ImplementTraits,
      Pass::DefineType,
      Pass::ValidateThrow,
    ].freeze

    COMPILE_PASSES = [
      *PASSES,
      Pass::GenerateTir,
      Pass::TailCallElimination,
      Pass::CodeGeneration,
    ].freeze

    attr_reader :state, :modules

    def initialize(state)
      @state = state
      @modules = []
    end

    def compile_main(path, output = nil)
      path = path.expand_path

      output ||= begin
        out_name = File.basename(path, '.*') + Config::BYTECODE_EXT
        File.join(Dir.pwd, out_name)
      end

      name = @state.main_module_name_from_path(path)
      main_mod = compile(name, path)

      if @state.config.compile?
        Codegen::Serializer.new(self, main_mod).serialize_to_file(output)
      end

      output
    end

    # name - The QualifiedName of the module.
    # path - The absolute file path of the module to compile, as a Pathname.
    def compile(name, path)
      passes = @state.config.compile? ? COMPILE_PASSES : PASSES
      mod = module_for_name_and_path(name, path)
      output = passes.reduce([]) do |input, klass|
        out = klass.new(self, mod).run(*input)

        break if out.nil? || state.diagnostics.errors?

        out
      end

      @modules.push(output.first) if output && @state.config.compile?

      mod
    end

    def module_for_name_and_path(name, path)
      location = SourceLocation.first_line(SourceFile.new(path))

      TIR::Module.new(name, location)
    end
  end
end
