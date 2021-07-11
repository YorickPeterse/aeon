# frozen_string_literal: true

module Inkoc
  module Pass
    # A compiler pass that simply marks a trait as implement. Trait
    # implementations are verified in a separate pass.
    class ImplementTraits
      include VisitorMethods
      include TypePass

      def on_body(node, scope)
        node.expressions.each do |expr|
          if expr.trait_implementation? || expr.variable_definition?
            define_type(expr, scope)
          end
        end

        nil
      end

      def on_trait_implementation(node, scope)
        object = define_type(node.object_name, scope)

        return object if object.error?

        unless object.object?
          diagnostics.not_an_object(
            node.object_name.name,
            object,
            node.location
          )

          return TypeSystem::Error.new
        end

        # The trait name has to be looked up in the context of the
        # implementation. This ensures that a Self type refers to the type
        # that the trait is implemented for, instead of referring to the type of
        # the outer scope.
        impl_block = TypeSystem::Block.closure(typedb.block_type)
        impl_scope = TypeScope
          .new(object, impl_block, @module, locals: node.body.locals)

        impl_scope.define_receiver_type

        trait = define_type_instance(node.trait_name, impl_scope)

        return trait if trait.error?

        object.implement_trait(trait)

        node.block_type = impl_block

        nil
      end

      # This is a bit of a hack so modules such as std::ffi can define constants
      # that use raw instructions, then re-open those in the same module.
      def on_define_variable(node, scope)
        return unless node.variable.constant?
        return unless node.value.send? && node.value.raw_instruction?

        value_type = define_type(node.value, scope)
        name = node.variable.name

        if scope.self_type.lookup_attribute(name).any?
          value_type = diagnostics
            .redefine_existing_constant_error(name, node.location)
        else
          scope.self_type.define_attribute(name, value_type)
        end

        store_type_as_global(name, value_type, scope, node.location)
        value_type
      end
    end
  end
end
