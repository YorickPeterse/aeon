# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class Trait
      include Equality
      include Type
      include TypeWithPrototype
      include TypeWithAttributes
      include GenericType
      include GenericTypeWithInstances
      include TypeName
      include NewInstance
      include WithoutEmptyTypeParameters

      attr_reader :name, :attributes, :type_parameters, :required_traits,
                  :unique_id, :default_traits

      attr_accessor :prototype, :type_parameter_instances, :required_methods

      def initialize(name: Config::TRAIT_CONST, prototype: nil, unique_id: nil)
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @type_parameters = TypeParameterTable.new
        @type_parameter_instances = TypeParameterInstances.new
        @required_traits = {}
        @default_traits = {}
        @required_methods = SymbolTable.new

        # A trait's unique ID is used by objects to quickly check if they
        # implement a certain trait, without having to rely on the (fully
        # qualified) name of the trait.
        @unique_id = unique_id
      end

      def trait?
        true
      end

      def generic_object?
        type_parameters.any?
      end

      def default_methods
        @attributes.symbols.select { |sym| sym.type.method? }
      end

      # Returns the method for the given name.
      #
      # name - The name of a method.
      def lookup_method(name)
        lookup_attribute(name)
          .or_else { required_methods[name] }
          .or_else { lookup_method_in_required_traits(name) }
          .or_else { lookup_method_in_default_traits(name) }
      end

      def lookup_method_in_required_traits(name)
        required_traits.each_value do |trait|
          if (sym = trait.lookup_method(name)) && sym.any?
            return sym
          end
        end

        NullSymbol.singleton
      end

      def lookup_method_in_default_traits(name)
        default_traits.each_value do |trait|
          if (sym = trait.lookup_method(name)) && sym.any?
            return sym
          end
        end

        NullSymbol.singleton
      end

      # Returns `true` if `self` is type compatible with `other`.
      #
      # other - The type to compare with.
      # rubocop: disable Metrics/CyclomaticComplexity
      # rubocop: disable Metrics/PerceivedComplexity
      def type_compatible?(other, state)
        return type_compatible?(other.type, state) if other.reference?
        return true if other.any? || self == other
        return implements_trait?(other, state) if other.trait?

        if other.type_parameter?
          return compatible_with_type_parameter?(other, state)
        end

        if other.generic_object?
          return true if compatible_with_generic_type?(other, state)
        end

        prototype_chain_compatible?(other)
      end
      # rubocop: enable Metrics/CyclomaticComplexity
      # rubocop: enable Metrics/PerceivedComplexity

      def compatible_with_type_parameter?(param, state)
        return true if param.required_traits.empty?

        param.required_traits.all? do |trait|
          implements_trait?(trait, state)
        end
      end

      def empty?
        required_traits.empty? && required_methods.empty?
      end

      def define_required_method(type)
        required_methods.define(type.name, type)
      end

      def implements_trait?(trait, state)
        if required_traits[trait.unique_id]
          true
        elsif default_traits[trait.unique_id]
          true
        elsif self == trait
          true
        elsif base_type == trait.base_type
          type_parameters.all? do |param|
            ours = lookup_type_parameter_instance(param)
            theirs = trait.lookup_type_parameter_instance(param)

            ours.type_compatible?(theirs, state)
          end
        else
          # The trait is not directly required, but might be required indirectly
          # via another required trait.
          required_traits.each_value.any? do |required|
            required.type_compatible?(trait, state)
          end
        end
      end

      def add_required_trait(trait)
        required_traits[trait.unique_id] = trait
      end

      def add_default_trait(trait)
        default_traits[trait.unique_id] = trait
      end

      def required_trait_types
        required_traits.values
      end
    end
  end
end
